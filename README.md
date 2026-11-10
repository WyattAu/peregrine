# Peregrine

A multi-platform Matrix client for the WyattAu estate, designed to replace
Element X, Cinny, FluffyChat, Nheko, Fractal, NeoChat, iamb, and gomuks as
the daily driver: correct E2EE, a real storage engine, deterministic
verification culture, and latency budgets enforced as release gates.

**Status: DESIGN (pre-code).** The goals corpus and architecture are in
`docs/`; nothing is implemented yet.

| Doc | Content |
|---|---|
| [docs/GOALS.md](docs/GOALS.md) | Mission, wedge analysis, principles, SLO budget |
| [docs/goals/catalog-1.md](docs/goals/catalog-1.md) | Foundations: DAG/state, data-oriented core, E2EE, sync, storage |
| [docs/goals/catalog-2.md](docs/goals/catalog-2.md) | Rendering, quant/IR, collaboration/CRDT, integrations |
| [docs/goals/catalog-3.md](docs/goals/catalog-3.md) | Audio, video, messaging core, AI/MCP, identity/IAM |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Crate topology, estate stack reuse, open decisions |
| [docs/ROADMAP.md](docs/ROADMAP.md) | Phases with exit gates, plan ledger |

Naming: the peregrine falcon is the fastest animal on Earth in its hunting
dive. This project is latency-first. Provisional; rename is cheap pre-scaffold.
