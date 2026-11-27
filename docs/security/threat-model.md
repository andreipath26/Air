# Initial threat model

Untrusted inputs include documents, model/teacher outputs, package manifests and learned executable procedures. Separate data from instructions; quarantine new records, validate provenance, enforce capability permissions, bound recursion/time/memory, never execute arbitrary learned code outside a sandbox, verify package integrity, use transactional versioned updates and rollback. No unrestricted ingestion or execution before adversarial and recovery tests. Implementation pending.
