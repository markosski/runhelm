## ADDED Requirements

### Requirement: Namespace-Aware Dispatch Tracking
`TaskDispatcher` SHALL share pending and active tracking across namespaces, SHALL retain namespace in pending dispatches and active leases, and SHALL validate the namespace echoed by worker results.

#### Scenario: Claimed dispatch includes namespace
- **WHEN** a worker claims pending work
- **THEN** the returned task dispatch includes the namespace supplied by the engine

#### Scenario: Active workflow limit uses composite identity
- **WHEN** two pending dispatches use the same workflow instance ID in different namespaces
- **THEN** an active lease for one does not block the other as the same workflow identity

#### Scenario: Matching namespace result completes dispatch
- **WHEN** a worker reports an active dispatch ID and the namespace matches the in-flight dispatch
- **THEN** the dispatcher completes the associated workflow-side waiter

#### Scenario: Mismatched namespace result is rejected
- **WHEN** a worker reports an active dispatch ID with a different namespace from the in-flight dispatch
- **THEN** the dispatcher does not complete or remove the active dispatch
