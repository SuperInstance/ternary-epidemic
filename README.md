# Ternary Epidemic — Gossip Protocol with Ternary Infection States

**Ternary Epidemic** implements an epidemic-style gossip protocol for GPU cluster state propagation using ternary infection states: **+1 (Infected)** has the update, **0 (Carrier)** is relaying but hasn't fully applied it, and **-1 (Susceptible)** needs the update. It provides push-pull gossip, convergence detection, and rumor mongering with anti-entropy synchronization.

## Why It Matters

Gossip protocols are the most robust way to propagate state across large distributed systems — they tolerate node failures, network partitions, and message loss with O(log N) convergence time. The ternary infection model adds a crucial intermediate state: carriers relay information while processing it, reducing the window of inconsistency compared to binary SI (Susceptible-Infected) models. For GPU clusters running ternary inference, this means model updates propagate faster and with fewer inconsistency windows. The push-pull mechanism ensures that even nodes behind firewalls (push-only) eventually receive updates through carrier intermediaries.

## How It Works

### Infection States

- **Susceptible (-1)**: Node hasn't received the update; its local state is stale
- **Carrier (0)**: Node has received the update and is relaying it, but hasn't fully applied/verified it yet
- **Infected (+1)**: Node has received, verified, and applied the update

State transitions: Susceptible → Carrier → Infected (monotonic progression per update).

### Push-Pull Gossip

Each round, every node contacts a random neighbor:
- **Push**: If the node has a newer version, it sends the update to the neighbor
- **Pull**: If the neighbor has a newer version, the node requests it

A node that receives an update transitions Susceptible → Carrier, then Carrier → Infected after applying it.

### Convergence Detection

The protocol tracks the fraction of nodes in each state. Convergence is achieved when >95% of nodes are Infected. Expected convergence time for N nodes with fanout F is:

```
E[rounds to convergence] ≈ log_{F+1}(N) + 3
```

For N = 1000 nodes with fanout 3, this is ~7 rounds.

### Rumor Mongering

To reduce message overhead, nodes can switch to rumor mongering after initial push-pull: each infected node contacts a small number of random neighbors per round and stops spreading after K rounds (configurable burnout).

## Quick Start

```rust
use ternary_epidemic::{GossipNode, InfectionState};

// Create a cluster of nodes
let mut nodes: Vec<GossipNode> = (0..100).map(|i| GossipNode::new(i)).collect();

// Add neighbor relationships (ring topology)
for i in 0..100 {
    nodes[i].add_neighbor((i + 1) % 100);
    nodes[i].add_neighbor((i + 99) % 100);
}

// Infect node 0 with version 1
nodes[0].state = InfectionState::Infected;
nodes[0].version = 1;

// Run gossip rounds
// After log(N) rounds, most nodes should be infected
```

```bash
cargo add ternary-epidemic
```

## API

| Type / Function | Description |
|---|---|
| `InfectionState` | `Susceptible(-1)`, `Carrier(0)`, `Infected(+1)` |
| `GossipNode` | `{ id, state, version, neighbors }` with `add_neighbor()` |
| Push-pull step | One gossip round per node |
| Convergence check | Fraction of Infected nodes |

## Architecture Notes

This is the state propagation layer of **SuperInstance**. Fleet-wide model updates, configuration changes, and γ/η rebalancing decisions spread via epidemic gossip. The carrier state (0) maps to the η term in γ + η = C: it represents the transient uncertainty during update propagation that resolves to either growth (Infected = +1) or stasis (Susceptible = -1). See [Architecture](https://github.com/SuperInstance/SuperInstance/blob/main/ARCHITECTURE.md).

## References

- Demers, Alan et al. "Epidemic Algorithms for Replicated Database Maintenance," *PODC*, 1987 — original gossip protocol.
- Kermack, W. O. & McKendrick, A. G. "A Contribution to the Mathematical Theory of Epidemics," *Proc. R. Soc. A*, 115(772), 1927 — SIR model.
- Jelasity, Márk et al. *Gossip-Based Computer Networking*, Oxford UP, 2011 — modern gossip protocols.

## License

MIT
