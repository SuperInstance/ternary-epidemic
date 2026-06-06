# ternary-epidemic

**How things spread in a world of three states. SIR models, cascades, and herd immunity on ternary networks.**

An epidemic is a cascade: one infected node infects its neighbors, who infect theirs, until the network is saturated or the disease burns out. The SIR model (Susceptible → Infected → Recovered) is the simplest epidemic model — and in ternary, the three states map directly to {-1, 0, +1}.

This crate implements SIR and SIS epidemic models on ternary networks, cascade detection (which seed nodes trigger the biggest outbreaks), tipping point analysis (what fraction must be infected to reach everyone), and herd immunity threshold computation (how many must be vaccinated to prevent spread).

## What's Inside

- **`SIRModel`** — SIR dynamics on arbitrary networks with ternary rates
- **`SISModel`** — SIS dynamics with majority-rule infection
- **`HealthState`** — Susceptible (0), Infected (1), Recovered (-1)
- **`find_tipping_point()`** — minimum fraction for global cascade
- **`cascade_sizes()`** — rank nodes by cascade size when seeded
- **`herd_immunity_threshold()`** — vaccination fraction to prevent spread

## The Deeper Truth

**Ternary epidemics are Boolean epidemics with a third state for "done."** The SIR model's Recovered state is the information-theoretic equivalent of "already processed and removed." In computing, this is exactly the garbage-collected version of an epidemic: Susceptible = unvisited, Infected = on the stack, Recovered = freed.

The tipping point — the critical fraction of initial infections that triggers a global cascade — is the network's epidemic threshold. In ternary, this threshold is determined entirely by the graph structure (degree distribution, clustering coefficient). The ternary constraint means the threshold is *higher* than in continuous models: you can't have "half-infected" nodes, so local outbreaks are more likely to die out before going global.

**Use cases:**
- **Information cascades** — how memes/news spread through social networks
- **Network resilience** — which nodes are most vulnerable to cascade failures
- **Vaccination strategy** — optimal vaccination to maximize herd immunity
- **Multi-agent failure** — cascading agent failures in fleet coordination
- **Education** — the simplest possible epidemic model

## See Also

- **ternary-network** — network topology and graph structure
- **ternary-diffusion** — diffusion processes on networks (continuous version)
- **ternary-cascade** — (if exists) dedicated cascade analysis
- **ternary-consensus** — consensus as anti-epidemic (convergence vs. spread)
- **ternary-trust** — trust dynamics can be modeled as SIS epidemics
- **ternary-graph** — graph algorithms underlying network structure

## Install

```bash
cargo add ternary-epidemic
```

## License

MIT
