# Project orientation

An interactive no-task launch receives a compact, provider-neutral orientation
before it starts. The orientation identifies the orchestrator, worker scope,
effective model defaults, concurrency settings, and the project selected by
the current working directory.

For a registered project it also lists only the configured, root-relative
locations for the project brain, agent instructions, orchestrator brain, and
planning home. The explicitly registered opener supplement may be included
when it is a regular file inside the project root, uses valid text, contains
no credential-shaped values, and fits the bounded read limit. Missing,
escaping, symlinked, oversized, or unsafe opener files are omitted.

The orientation is generated at launch time. It does not alter an explicit
initial task, read provider credentials, crawl the project, or start a provider
request. The canonical agent-tool manifest instruction remains included so the
orchestrator can use the same Neomax command surface as delegated workers.

`neomax orient` prints the same orientation on demand inside an orchestrator
session. `neomax orient --hook` emits the SessionStart JSON shape and is silent
outside an interactive orchestrator. The launch path supplies the generated
orientation only for a no-task root session; task prompts remain unchanged.
