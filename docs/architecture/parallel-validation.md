# Parallel validation lanes

Six independent GitHub Actions jobs validate core correctness, knowledge/storage, cognition/language, compiler/security, AMT, and performance. No job depends on another runner's local files. Shared prerequisite fixtures are recreated in each lane. All original CI checks are retained; failures are independently visible. The separate real-model experiment workflow remains an optional seventh lane. These are concurrent validation workers, not autonomous coding agents. Scientific claims require inspecting experiment outputs, not merely workflow success.
