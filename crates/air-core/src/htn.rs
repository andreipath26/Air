//! Deterministic hierarchical task decomposition and bounded execution.
//!
//! The planner emits a small DAG, not model-generated hidden reasoning. Work is
//! delegated through a caller-supplied sandbox runner and verifier. This crate
//! does not launch processes, assume `/dev/shm`, or claim that an arbitrary
//! implementation of `SandboxedTaskRunner` is isolated.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

pub const MAX_TASKS: usize = 64;
pub const MAX_GOAL_BYTES: usize = 16 * 1024;
pub const MAX_NODE_INPUT_BYTES: usize = 32 * 1024;
pub const MAX_NODE_OUTPUT_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TaskId(pub u16);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TaskKind {
    CargoDependencies,
    DataStructuresAndTraits,
    StateLogic,
    IntegrationTests,
}

impl TaskKind {
    fn tag(self) -> u8 {
        match self {
            Self::CargoDependencies => 1,
            Self::DataStructuresAndTraits => 2,
            Self::StateLogic => 3,
            Self::IntegrationTests => 4,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::CargoDependencies => "Cargo.toml dependencies",
            Self::DataStructuresAndTraits => "core data structs and traits",
            Self::StateLogic => "state and logic implementation",
            Self::IntegrationTests => "integration tests",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskNode {
    pub id: TaskId,
    pub kind: TaskKind,
    /// Direct prerequisite task IDs. Order is canonicalized by TaskDag::new.
    pub depends_on: Vec<TaskId>,
    pub max_input_bytes: usize,
    pub max_output_bytes: usize,
}

impl TaskNode {
    pub fn new(id: TaskId, kind: TaskKind, depends_on: Vec<TaskId>) -> Self {
        Self {
            id,
            kind,
            depends_on,
            max_input_bytes: MAX_NODE_INPUT_BYTES,
            max_output_bytes: MAX_NODE_OUTPUT_BYTES,
        }
    }

    pub fn with_limits(mut self, max_input_bytes: usize, max_output_bytes: usize) -> Self {
        self.max_input_bytes = max_input_bytes;
        self.max_output_bytes = max_output_bytes;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanError {
    EmptyPlan,
    TooManyTasks,
    DuplicateTask(TaskId),
    MissingDependency { task: TaskId, dependency: TaskId },
    SelfDependency(TaskId),
    DuplicateDependency { task: TaskId, dependency: TaskId },
    InvalidLimits(TaskId),
    Cycle,
    GoalTooLarge,
    CheckpointForDifferentPlan,
    CheckpointUnknownTask(TaskId),
    CheckpointOutputTooLarge(TaskId),
    CheckpointMissingPrerequisite { task: TaskId, dependency: TaskId },
}

impl std::fmt::Display for PlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyPlan => write!(f, "task plan is empty"),
            Self::TooManyTasks => write!(f, "task plan exceeds task limit"),
            Self::DuplicateTask(id) => write!(f, "duplicate task {}", id.0),
            Self::MissingDependency { task, dependency } => {
                write!(f, "task {} depends on missing task {}", task.0, dependency.0)
            }
            Self::SelfDependency(id) => write!(f, "task {} depends on itself", id.0),
            Self::DuplicateDependency { task, dependency } => write!(
                f,
                "task {} repeats dependency {}",
                task.0, dependency.0
            ),
            Self::InvalidLimits(id) => write!(f, "task {} has invalid byte limits", id.0),
            Self::Cycle => write!(f, "task plan contains a dependency cycle"),
            Self::GoalTooLarge => write!(f, "task goal exceeds byte limit"),
            Self::CheckpointForDifferentPlan => {
                write!(f, "checkpoint belongs to a different task plan")
            }
            Self::CheckpointUnknownTask(id) => {
                write!(f, "checkpoint contains unknown task {}", id.0)
            }
            Self::CheckpointOutputTooLarge(id) => {
                write!(f, "checkpoint output for task {} exceeds its limit", id.0)
            }
            Self::CheckpointMissingPrerequisite { task, dependency } => write!(
                f,
                "checkpoint task {} is missing prerequisite {}",
                task.0, dependency.0
            ),
        }
    }
}

impl std::error::Error for PlanError {}

/// A validated DAG. Nodes are stored and traversed in stable TaskId order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskDag {
    nodes: BTreeMap<TaskId, TaskNode>,
    fingerprint: u64,
}

impl TaskDag {
    pub fn new(mut nodes: Vec<TaskNode>) -> Result<Self, PlanError> {
        if nodes.is_empty() {
            return Err(PlanError::EmptyPlan);
        }
        if nodes.len() > MAX_TASKS {
            return Err(PlanError::TooManyTasks);
        }

        let mut by_id = BTreeMap::new();
        for node in &mut nodes {
            node.depends_on.sort_unstable();
            if node.max_input_bytes == 0
                || node.max_input_bytes > MAX_NODE_INPUT_BYTES
                || node.max_output_bytes == 0
                || node.max_output_bytes > MAX_NODE_OUTPUT_BYTES
            {
                return Err(PlanError::InvalidLimits(node.id));
            }
            if node.depends_on.windows(2).any(|pair| pair[0] == pair[1]) {
                let dependency = node
                    .depends_on
                    .windows(2)
                    .find(|pair| pair[0] == pair[1])
                    .map(|pair| pair[0])
                    .expect("duplicate dependency was found");
                return Err(PlanError::DuplicateDependency {
                    task: node.id,
                    dependency,
                });
            }
            if by_id.insert(node.id, node.clone()).is_some() {
                return Err(PlanError::DuplicateTask(node.id));
            }
        }

        for node in by_id.values() {
            for dependency in &node.depends_on {
                if *dependency == node.id {
                    return Err(PlanError::SelfDependency(node.id));
                }
                if !by_id.contains_key(dependency) {
                    return Err(PlanError::MissingDependency {
                        task: node.id,
                        dependency: *dependency,
                    });
                }
            }
        }

        let mut plan = Self {
            nodes: by_id,
            fingerprint: 0,
        };
        plan.topological_order()?;
        plan.fingerprint = plan.compute_fingerprint();
        Ok(plan)
    }

    /// Canonical four-step decomposition for a bounded coding change.
    pub fn coding_change() -> Self {
        let nodes = vec![
            TaskNode::new(TaskId(1), TaskKind::CargoDependencies, vec![]),
            TaskNode::new(
                TaskId(2),
                TaskKind::DataStructuresAndTraits,
                vec![TaskId(1)],
            ),
            TaskNode::new(TaskId(3), TaskKind::StateLogic, vec![TaskId(2)]),
            TaskNode::new(TaskId(4), TaskKind::IntegrationTests, vec![TaskId(3)]),
        ];
        // This plan is static and its invariants are encoded above.
        Self::new(nodes).expect("static coding-change DAG is valid")
    }

    pub fn node(&self, id: TaskId) -> Option<&TaskNode> {
        self.nodes.get(&id)
    }

    pub fn nodes(&self) -> impl Iterator<Item = &TaskNode> {
        self.nodes.values()
    }

    pub fn topological_order(&self) -> Result<Vec<TaskId>, PlanError> {
        let mut in_degree: BTreeMap<TaskId, usize> = self
            .nodes
            .iter()
            .map(|(id, node)| (*id, node.depends_on.len()))
            .collect();
        let mut successors: BTreeMap<TaskId, Vec<TaskId>> =
            self.nodes.keys().map(|id| (*id, Vec::new())).collect();

        for node in self.nodes.values() {
            for dependency in &node.depends_on {
                successors
                    .get_mut(dependency)
                    .expect("dependencies were validated")
                    .push(node.id);
            }
        }
        for children in successors.values_mut() {
            children.sort_unstable();
        }

        let mut ready: BTreeSet<TaskId> = in_degree
            .iter()
            .filter_map(|(id, degree)| (*degree == 0).then_some(*id))
            .collect();
        let mut order = Vec::with_capacity(self.nodes.len());

        while let Some(id) = ready.iter().next().copied() {
            ready.remove(&id);
            order.push(id);
            for child in successors.get(&id).expect("every node has successors") {
                let degree = in_degree.get_mut(child).expect("every node has a degree");
                *degree -= 1;
                if *degree == 0 {
                    ready.insert(*child);
                }
            }
        }

        if order.len() != self.nodes.len() {
            return Err(PlanError::Cycle);
        }
        Ok(order)
    }

    fn compute_fingerprint(&self) -> u64 {
        // Stable FNV-1a rather than hashing with a process-randomized state.
        let mut hash = 0xcbf29ce484222325u64;
        let mut feed = |bytes: &[u8]| {
            for byte in bytes {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(0x100000001b3);
            }
        };
        for node in self.nodes.values() {
            feed(&node.id.0.to_le_bytes());
            feed(&[node.kind.tag()]);
            feed(&(node.max_input_bytes as u64).to_le_bytes());
            feed(&(node.max_output_bytes as u64).to_le_bytes());
            for dependency in &node.depends_on {
                feed(&dependency.0.to_le_bytes());
            }
            feed(&[0xff]);
        }
        hash
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyArtifact {
    pub task_id: TaskId,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskInput {
    pub goal: String,
    /// Only direct prerequisite outputs are forwarded to this node.
    pub dependencies: Vec<DependencyArtifact>,
}

impl TaskInput {
    pub fn encoded_len(&self) -> Option<usize> {
        self.dependencies.iter().try_fold(self.goal.len(), |total, item| {
            total.checked_add(2)?.checked_add(item.bytes.len())
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskCheckpoint {
    fingerprint: u64,
    artifacts: BTreeMap<TaskId, Vec<u8>>,
}

impl TaskCheckpoint {
    pub fn new(plan: &TaskDag) -> Self {
        Self {
            fingerprint: plan.fingerprint,
            artifacts: BTreeMap::new(),
        }
    }

    pub fn artifact(&self, task_id: TaskId) -> Option<&[u8]> {
        self.artifacts.get(&task_id).map(Vec::as_slice)
    }

    pub fn completed_tasks(&self) -> impl Iterator<Item = TaskId> + '_ {
        self.artifacts.keys().copied()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionStatus {
    Complete,
    Failed { task_id: TaskId, reason: String },
    Cancelled { next_task: Option<TaskId> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionReport {
    pub checkpoint: TaskCheckpoint,
    /// Tasks completed during this invocation, in execution order.
    pub completed_now: Vec<TaskId>,
    pub status: ExecutionStatus,
}

#[derive(Debug, Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Implementations must bound inputs and execute each node inside an isolated,
/// offline sandbox. The trait itself is only a contract; air-core does not
/// create a `/dev/shm` workspace or claim process isolation.
pub trait SandboxedTaskRunner {
    fn run_sandboxed(
        &mut self,
        task: &TaskNode,
        input: &TaskInput,
    ) -> Result<Vec<u8>, String>;
}

/// Accepts or rejects a sandbox-produced result before it enters the checkpoint.
pub trait TaskOutputVerifier {
    fn verify(&self, task: &TaskNode, input: &TaskInput, output: &[u8]) -> Result<(), String>;
}

/// Executes ready DAG nodes sequentially, forwarding verified outputs only to
/// dependent nodes. On failure or cancellation, the verified prefix is retained
/// so a caller can resume from the returned checkpoint.
pub fn execute(
    plan: &TaskDag,
    goal: &str,
    checkpoint: TaskCheckpoint,
    cancellation: &CancellationToken,
    runner: &mut impl SandboxedTaskRunner,
    verifier: &impl TaskOutputVerifier,
) -> Result<ExecutionReport, PlanError> {
    if goal.is_empty() || goal.len() > MAX_GOAL_BYTES {
        return Err(PlanError::GoalTooLarge);
    }
    if checkpoint.fingerprint != plan.fingerprint {
        return Err(PlanError::CheckpointForDifferentPlan);
    }
    validate_checkpoint(plan, &checkpoint)?;

    let order = plan.topological_order()?;
    let mut checkpoint = checkpoint;
    let mut completed_now = Vec::new();

    for task_id in order {
        if checkpoint.artifacts.contains_key(&task_id) {
            continue;
        }
        if cancellation.is_cancelled() {
            return Ok(ExecutionReport {
                checkpoint,
                completed_now,
                status: ExecutionStatus::Cancelled {
                    next_task: Some(task_id),
                },
            });
        }

        let task = plan.node(task_id).expect("ordered node exists");
        let dependencies: Vec<DependencyArtifact> = task
            .depends_on
            .iter()
            .filter_map(|dependency| {
                checkpoint.artifacts.get(dependency).map(|bytes| DependencyArtifact {
                    task_id: *dependency,
                    bytes: bytes.clone(),
                })
            })
            .collect();

        // Missing prerequisites can only result from an invalid checkpoint.
        if dependencies.len() != task.depends_on.len() {
            let dependency = task
                .depends_on
                .iter()
                .find(|dependency| !checkpoint.artifacts.contains_key(dependency))
                .copied()
                .expect("dependency count mismatch implies missing item");
            return Err(PlanError::CheckpointMissingPrerequisite {
                task: task_id,
                dependency,
            });
        }

        let input = TaskInput {
            goal: goal.to_owned(),
            dependencies,
        };
        let input_len = input.encoded_len().unwrap_or(usize::MAX);
        if input_len > task.max_input_bytes {
            return Ok(ExecutionReport {
                checkpoint,
                completed_now,
                status: ExecutionStatus::Failed {
                    task_id,
                    reason: "task input exceeds byte limit".into(),
                },
            });
        }

        let output = match runner.run_sandboxed(task, &input) {
            Ok(output) => output,
            Err(reason) => {
                return Ok(ExecutionReport {
                    checkpoint,
                    completed_now,
                    status: ExecutionStatus::Failed { task_id, reason },
                })
            }
        };
        if output.len() > task.max_output_bytes {
            return Ok(ExecutionReport {
                checkpoint,
                completed_now,
                status: ExecutionStatus::Failed {
                    task_id,
                    reason: "task output exceeds byte limit".into(),
                },
            });
        }
        if let Err(reason) = verifier.verify(task, &input, &output) {
            return Ok(ExecutionReport {
                checkpoint,
                completed_now,
                status: ExecutionStatus::Failed { task_id, reason },
            });
        }

        checkpoint.artifacts.insert(task_id, output);
        completed_now.push(task_id);
    }

    Ok(ExecutionReport {
        checkpoint,
        completed_now,
        status: ExecutionStatus::Complete,
    })
}

fn validate_checkpoint(plan: &TaskDag, checkpoint: &TaskCheckpoint) -> Result<(), PlanError> {
    for (task_id, output) in &checkpoint.artifacts {
        let task = plan
            .node(*task_id)
            .ok_or(PlanError::CheckpointUnknownTask(*task_id))?;
        if output.len() > task.max_output_bytes {
            return Err(PlanError::CheckpointOutputTooLarge(*task_id));
        }
        for dependency in &task.depends_on {
            if !checkpoint.artifacts.contains_key(dependency) {
                return Err(PlanError::CheckpointMissingPrerequisite {
                    task: *task_id,
                    dependency: *dependency,
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct EchoRunner {
        calls: Vec<(TaskId, Vec<TaskId>)>,
        fail_once: Option<TaskId>,
        output: Vec<u8>,
    }

    impl SandboxedTaskRunner for EchoRunner {
        fn run_sandboxed(
            &mut self,
            task: &TaskNode,
            input: &TaskInput,
        ) -> Result<Vec<u8>, String> {
            self.calls.push((
                task.id,
                input.dependencies.iter().map(|item| item.task_id).collect(),
            ));
            if self.fail_once == Some(task.id) {
                self.fail_once = None;
                return Err("injected failure".into());
            }
            if self.output.is_empty() {
                let mut output = task.kind.label().as_bytes().to_vec();
                for dependency in &input.dependencies {
                    output.extend_from_slice(&dependency.bytes);
                }
                Ok(output)
            } else {
                Ok(self.output.clone())
            }
        }
    }

    struct AcceptAll;
    impl TaskOutputVerifier for AcceptAll {
        fn verify(&self, _task: &TaskNode, _input: &TaskInput, _output: &[u8]) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn coding_change_dag_is_stable_and_dependency_ordered() {
        let plan = TaskDag::coding_change();
        let expected = vec![TaskId(1), TaskId(2), TaskId(3), TaskId(4)];
        assert_eq!(plan.topological_order().unwrap(), expected);
        assert_eq!(plan.topological_order().unwrap(), expected);
        assert_eq!(
            plan.nodes().map(|node| node.kind).collect::<Vec<_>>(),
            vec![
                TaskKind::CargoDependencies,
                TaskKind::DataStructuresAndTraits,
                TaskKind::StateLogic,
                TaskKind::IntegrationTests,
            ]
        );
    }

    #[test]
    fn rejects_missing_dependencies_and_cycles() {
        let missing = TaskDag::new(vec![TaskNode::new(
            TaskId(1),
            TaskKind::StateLogic,
            vec![TaskId(99)],
        )]);
        assert_eq!(
            missing.unwrap_err(),
            PlanError::MissingDependency {
                task: TaskId(1),
                dependency: TaskId(99),
            }
        );

        let cyclic = TaskDag::new(vec![
            TaskNode::new(TaskId(1), TaskKind::StateLogic, vec![TaskId(2)]),
            TaskNode::new(TaskId(2), TaskKind::IntegrationTests, vec![TaskId(1)]),
        ]);
        assert_eq!(cyclic.unwrap_err(), PlanError::Cycle);
    }

    #[test]
    fn sequential_execution_forwards_verified_dependency_outputs() {
        let plan = TaskDag::coding_change();
        let mut runner = EchoRunner::default();
        let report = execute(
            &plan,
            "Implement a small bounded change",
            TaskCheckpoint::new(&plan),
            &CancellationToken::default(),
            &mut runner,
            &AcceptAll,
        )
        .unwrap();

        assert_eq!(report.status, ExecutionStatus::Complete);
        assert_eq!(
            report.completed_now,
            vec![TaskId(1), TaskId(2), TaskId(3), TaskId(4)]
        );
        assert_eq!(
            runner.calls,
            vec![
                (TaskId(1), vec![]),
                (TaskId(2), vec![TaskId(1)]),
                (TaskId(3), vec![TaskId(2)]),
                (TaskId(4), vec![TaskId(3)]),
            ]
        );
        assert!(report.checkpoint.artifact(TaskId(4)).is_some());
    }

    #[test]
    fn failed_step_preserves_verified_prefix_and_can_resume() {
        let plan = TaskDag::coding_change();
        let mut runner = EchoRunner {
            fail_once: Some(TaskId(3)),
            ..EchoRunner::default()
        };
        let first = execute(
            &plan,
            "Implement a small bounded change",
            TaskCheckpoint::new(&plan),
            &CancellationToken::default(),
            &mut runner,
            &AcceptAll,
        )
        .unwrap();

        assert_eq!(
            first.status,
            ExecutionStatus::Failed {
                task_id: TaskId(3),
                reason: "injected failure".into(),
            }
        );
        assert_eq!(
            first.checkpoint.completed_tasks().collect::<Vec<_>>(),
            vec![TaskId(1), TaskId(2)]
        );

        let resumed = execute(
            &plan,
            "Implement a small bounded change",
            first.checkpoint,
            &CancellationToken::default(),
            &mut runner,
            &AcceptAll,
        )
        .unwrap();
        assert_eq!(resumed.status, ExecutionStatus::Complete);
        assert_eq!(
            resumed.completed_now,
            vec![TaskId(3), TaskId(4)]
        );
        assert_eq!(
            runner.calls.iter().map(|call| call.0).collect::<Vec<_>>(),
            vec![TaskId(1), TaskId(2), TaskId(3), TaskId(3), TaskId(4)]
        );
    }

    #[test]
    fn enforces_goal_input_and_output_limits() {
        let input_limited = TaskDag::new(vec![TaskNode::new(
            TaskId(1),
            TaskKind::CargoDependencies,
            vec![],
        )
        .with_limits(8, 8)])
        .unwrap();
        let mut runner = EchoRunner::default();
        let input_report = execute(
            &input_limited,
            "goal exceeds",
            TaskCheckpoint::new(&input_limited),
            &CancellationToken::default(),
            &mut runner,
            &AcceptAll,
        )
        .unwrap();
        assert!(matches!(
            input_report.status,
            ExecutionStatus::Failed { task_id: TaskId(1), .. }
        ));
        assert!(runner.calls.is_empty());

        let output_limited = TaskDag::new(vec![TaskNode::new(
            TaskId(1),
            TaskKind::CargoDependencies,
            vec![],
        )
        .with_limits(64, 4)])
        .unwrap();
        runner.output = b"too large".to_vec();
        let output_report = execute(
            &output_limited,
            "goal",
            TaskCheckpoint::new(&output_limited),
            &CancellationToken::default(),
            &mut runner,
            &AcceptAll,
        )
        .unwrap();
        assert!(matches!(
            output_report.status,
            ExecutionStatus::Failed { task_id: TaskId(1), .. }
        ));
        assert!(output_report.checkpoint.artifact(TaskId(1)).is_none());
    }

    #[test]
    fn cancellation_preserves_checkpoint_before_next_node() {
        let plan = TaskDag::coding_change();
        let token = CancellationToken::default();
        token.cancel();
        let mut runner = EchoRunner::default();
        let report = execute(
            &plan,
            "Implement a small bounded change",
            TaskCheckpoint::new(&plan),
            &token,
            &mut runner,
            &AcceptAll,
        )
        .unwrap();
        assert_eq!(
            report.status,
            ExecutionStatus::Cancelled {
                next_task: Some(TaskId(1))
            }
        );
        assert!(report.completed_now.is_empty());
        assert!(runner.calls.is_empty());
    }
}
