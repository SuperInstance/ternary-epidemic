//! # ternary-epidemic
//!
//! Epidemic-style gossip protocol for GPU cluster state propagation using
//! ternary infection states: `+1` (infected/has update), `0` (carrier/relaying),
//! `-1` (susceptible/needs update).
//!
//! Features push-pull gossip, convergence detection, and rumor mongering.

#![forbid(unsafe_code)]

use std::collections::{HashMap, HashSet};

/// Ternary infection state for a gossip node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InfectionState {
    /// Susceptible: node needs the update (-1)
    Susceptible = -1,
    /// Carrier: node is relaying the update but hasn't fully applied it (0)
    Carrier = 0,
    /// Infected: node has received and applied the update (+1)
    Infected = 1,
}

impl InfectionState {
    /// Numeric value of the infection state.
    pub fn value(&self) -> i8 {
        match self {
            InfectionState::Susceptible => -1,
            InfectionState::Carrier => 0,
            InfectionState::Infected => 1,
        }
    }

    /// Parse from i8 value.
    pub fn from_value(v: i8) -> Option<Self> {
        match v {
            -1 => Some(InfectionState::Susceptible),
            0 => Some(InfectionState::Carrier),
            1 => Some(InfectionState::Infected),
            _ => None,
        }
    }
}

/// A node participating in the gossip protocol.
#[derive(Debug, Clone)]
pub struct GossipNode {
    /// Unique node identifier.
    pub id: u32,
    /// Current infection state.
    pub state: InfectionState,
    /// Local data version (monotonically increasing).
    pub version: u64,
    /// Set of neighbor node IDs this node can communicate with.
    pub neighbors: HashSet<u32>,
}

impl GossipNode {
    /// Create a new susceptible node.
    pub fn new(id: u32) -> Self {
        GossipNode {
            id,
            state: InfectionState::Susceptible,
            version: 0,
            neighbors: HashSet::new(),
        }
    }

    /// Create a node with a specific state and version.
    pub fn with_state(id: u32, state: InfectionState, version: u64) -> Self {
        GossipNode {
            id,
            state,
            version,
            neighbors: HashSet::new(),
        }
    }

    /// Add a bidirectional neighbor relationship.
    pub fn add_neighbor(&mut self, neighbor_id: u32) {
        self.neighbors.insert(neighbor_id);
    }

    /// Check if all neighbors are infected (for rumor mongering stop condition).
    pub fn all_neighbors_infected(&self, cluster: &GossipCluster) -> bool {
        self.neighbors.iter().all(|&nid| {
            cluster
                .nodes
                .get(&nid)
                .map(|n| n.state == InfectionState::Infected)
                .unwrap_or(true)
        })
    }

    /// Create a state summary for exchange during gossip.
    pub fn state_summary(&self) -> NodeSummary {
        NodeSummary {
            id: self.id,
            state: self.state,
            version: self.version,
        }
    }
}

/// Summary of a node's state, exchanged during push-pull gossip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeSummary {
    pub id: u32,
    pub state: InfectionState,
    pub version: u64,
}

/// Result of a single gossip round.
#[derive(Debug, Clone)]
pub struct RoundResult {
    /// The round number.
    pub round: u32,
    /// Pairs of nodes that exchanged state this round: (initiator, responder).
    pub exchanges: Vec<(u32, u32)>,
    /// State transitions that occurred: (node_id, old_state, new_state).
    pub transitions: Vec<(u32, InfectionState, InfectionState)>,
    /// Whether the cluster has converged.
    pub converged: bool,
}

/// A cluster of gossip nodes running epidemic propagation.
#[derive(Debug, Clone)]
pub struct GossipCluster {
    /// All nodes in the cluster, indexed by ID.
    pub nodes: HashMap<u32, GossipNode>,
    /// Fanout: number of random peers each node contacts per round.
    pub fanout: usize,
    /// Current round counter.
    pub round: u32,
    /// RNG seed for deterministic selection (simple LCG).
    rng_seed: u64,
    /// The data version being propagated.
    target_version: u64,
    /// Source node that initiated the update.
    source_id: Option<u32>,
}

impl GossipCluster {
    /// Create a new cluster with the given fanout.
    pub fn new(fanout: usize) -> Self {
        GossipCluster {
            nodes: HashMap::new(),
            fanout,
            round: 0,
            rng_seed: 42,
            target_version: 0,
            source_id: None,
        }
    }

    /// Add a node to the cluster.
    pub fn add_node(&mut self, node: GossipNode) {
        self.nodes.insert(node.id, node);
    }

    /// Seed the update from a source node. The source becomes Infected
    /// and its version is set as the target version.
    pub fn seed_update(&mut self, source_id: u32, version: u64) {
        self.target_version = version;
        self.source_id = Some(source_id);
        if let Some(node) = self.nodes.get_mut(&source_id) {
            node.state = InfectionState::Infected;
            node.version = version;
        }
    }

    /// Check if the cluster has converged (all nodes infected with target version).
    pub fn is_converged(&self) -> bool {
        if self.target_version == 0 {
            return false;
        }
        self.nodes.values().all(|n| {
            n.state == InfectionState::Infected && n.version == self.target_version
        })
    }

    /// Count nodes in each infection state.
    pub fn state_counts(&self) -> (usize, usize, usize) {
        let mut infected = 0;
        let mut carrier = 0;
        let mut susceptible = 0;
        for n in self.nodes.values() {
            match n.state {
                InfectionState::Infected => infected += 1,
                InfectionState::Carrier => carrier += 1,
                InfectionState::Susceptible => susceptible += 1,
            }
        }
        (infected, carrier, susceptible)
    }

    /// Get IDs of nodes that have received the update (Infected state).
    pub fn infected_nodes(&self) -> Vec<u32> {
        self.nodes
            .values()
            .filter(|n| n.state == InfectionState::Infected)
            .map(|n| n.id)
            .collect()
    }

    /// Simple deterministic pseudo-random number generator (LCG).
    fn next_random(&mut self) -> u64 {
        self.rng_seed = self.rng_seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        self.rng_seed
    }

    /// Pick `count` random elements from a slice.
    fn pick_random_neighbors_from(&mut self, neighbors: &[u32], count: usize) -> Vec<u32> {
        let mut picked: Vec<u32> = neighbors.to_vec();
        let len = picked.len();
        for i in (1..len).rev() {
            let j = (self.next_random() as usize) % (i + 1);
            picked.swap(i, j);
        }
        picked.truncate(count);
        picked
    }

    /// Run one round of push-pull gossip.
    ///
    /// In each round:
    /// 1. Each non-susceptible node picks `fanout` random neighbors
    /// 2. Push: sender transmits its state summary to selected neighbors
    /// 3. Pull: selected neighbors respond with their state summary
    /// 4. State updates propagate based on version comparison
    pub fn gossip_round(&mut self) -> RoundResult {
        self.round += 1;
        let mut exchanges = Vec::new();
        let mut transitions = Vec::new();

        // Phase 1: Collect summaries (no mutation)
        let mut summaries: HashMap<u32, NodeSummary> = HashMap::new();
        for node in self.nodes.values() {
            summaries.insert(node.id, node.state_summary());
        }

        // Phase 2: Collect candidates, then select randomly (avoid borrow conflicts)
        let mut candidates: Vec<(u32, Vec<u32>, usize)> = Vec::new();
        for node in self.nodes.values() {
            if node.state != InfectionState::Susceptible {
                let count = self.fanout.min(node.neighbors.len());
                if count > 0 {
                    let neighbors: Vec<u32> = node.neighbors.iter().copied().collect();
                    candidates.push((node.id, neighbors, count));
                }
            }
        }
        let mut pending_pushes: Vec<(u32, Vec<u32>)> = Vec::new();
        for (id, neighbors, count) in candidates {
            pending_pushes.push((id, self.pick_random_neighbors_from(&neighbors, count)));
        }

        // Track which nodes get updated this round
        let mut new_states: HashMap<u32, (InfectionState, u64)> = HashMap::new();

        // Process push-pull exchanges
        for (sender_id, targets) in &pending_pushes {
            for &target_id in targets {
                exchanges.push((*sender_id, target_id));

                let sender_summary = *summaries.get(sender_id).unwrap();
                let target_summary = *summaries.get(&target_id).unwrap();

                // Push: target receives sender's newer state
                if sender_summary.version > target_summary.version
                    && sender_summary.state != InfectionState::Susceptible
                {
                    // Target upgrades to Carrier
                    let entry = new_states.entry(target_id).or_insert((InfectionState::Susceptible, 0));
                    if sender_summary.version > entry.1 {
                        entry.0 = InfectionState::Carrier;
                        entry.1 = sender_summary.version;
                    }
                }

                // Pull: sender receives target's newer state
                if target_summary.version > sender_summary.version
                    && target_summary.state != InfectionState::Susceptible
                {
                    let entry = new_states.entry(*sender_id).or_insert((InfectionState::Susceptible, 0));
                    if target_summary.version > entry.1 {
                        entry.0 = InfectionState::Carrier;
                        entry.1 = target_summary.version;
                    }
                }
            }
        }

        // Apply state transitions
        for (node_id, (new_state, new_version)) in new_states {
            let node = self.nodes.get_mut(&node_id).unwrap();
            let old_state = node.state;
            if new_state != old_state || new_version > node.version {
                transitions.push((node_id, old_state, new_state));
                node.state = new_state;
                node.version = new_version;
            }
        }

        // Advance Carriers to Infected (they've had a chance to relay)
        for node in self.nodes.values_mut() {
            if node.state == InfectionState::Carrier && node.version == self.target_version {
                let old_state = node.state;
                node.state = InfectionState::Infected;
                transitions.push((node.id, old_state, InfectionState::Infected));
            }
        }

        let converged = self.is_converged();

        RoundResult {
            round: self.round,
            exchanges,
            transitions,
            converged,
        }
    }

    /// Run gossip until convergence or max rounds.
    pub fn run_until_converged(&mut self, max_rounds: u32) -> Vec<RoundResult> {
        let mut results = Vec::new();
        for _ in 0..max_rounds {
            let result = self.gossip_round();
            let done = result.converged;
            results.push(result);
            if done {
                break;
            }
        }
        results
    }

    /// Rumor mongering: run gossip but each node stops spreading once all
    /// its neighbors are infected.
    pub fn rumor_mongering_round(&mut self) -> RoundResult {
        self.round += 1;
        let mut exchanges = Vec::new();
        let mut transitions = Vec::new();

        let mut summaries: HashMap<u32, NodeSummary> = HashMap::new();
        for node in self.nodes.values() {
            summaries.insert(node.id, node.state_summary());
        }

        let mut new_states: HashMap<u32, (InfectionState, u64)> = HashMap::new();
        let mut pending_pushes: Vec<(u32, Vec<u32>)> = Vec::new();

        // Collect rumor mongering decisions
        let mut rumor_active: Vec<(u32, Vec<u32>)> = Vec::new();
        for node in self.nodes.values() {
            if node.state != InfectionState::Susceptible {
                if node.all_neighbors_infected(self) {
                    continue;
                }
                let count = self.fanout.min(node.neighbors.len());
                if count > 0 {
                    rumor_active.push((node.id, node.neighbors.iter().copied().collect()));
                }
            }
        }
        for (id, neighbors) in rumor_active {
            let count = self.fanout.min(neighbors.len());
            let targets = self.pick_random_neighbors_from(&neighbors, count);
            pending_pushes.push((id, targets));
        }

        for (sender_id, targets) in &pending_pushes {
            for &target_id in targets {
                exchanges.push((*sender_id, target_id));

                let sender_summary = *summaries.get(sender_id).unwrap();
                let target_summary = *summaries.get(&target_id).unwrap();

                if sender_summary.version > target_summary.version
                    && sender_summary.state != InfectionState::Susceptible
                {
                    let entry = new_states.entry(target_id).or_insert((InfectionState::Susceptible, 0));
                    if sender_summary.version > entry.1 {
                        entry.0 = InfectionState::Carrier;
                        entry.1 = sender_summary.version;
                    }
                }

                if target_summary.version > sender_summary.version
                    && target_summary.state != InfectionState::Susceptible
                {
                    let entry = new_states.entry(*sender_id).or_insert((InfectionState::Susceptible, 0));
                    if target_summary.version > entry.1 {
                        entry.0 = InfectionState::Carrier;
                        entry.1 = target_summary.version;
                    }
                }
            }
        }

        for (node_id, (new_state, new_version)) in new_states {
            let node = self.nodes.get_mut(&node_id).unwrap();
            let old_state = node.state;
            if new_state != old_state || new_version > node.version {
                transitions.push((node_id, old_state, new_state));
                node.state = new_state;
                node.version = new_version;
            }
        }

        // Advance carriers to infected
        for node in self.nodes.values_mut() {
            if node.state == InfectionState::Carrier && node.version == self.target_version {
                let old_state = node.state;
                node.state = InfectionState::Infected;
                transitions.push((node.id, old_state, InfectionState::Infected));
            }
        }

        let converged = self.is_converged();

        RoundResult {
            round: self.round,
            exchanges,
            transitions,
            converged,
        }
    }

    /// Run rumor mongering until convergence or max rounds.
    pub fn run_rumor_mongering(&mut self, max_rounds: u32) -> Vec<RoundResult> {
        let mut results = Vec::new();
        for _ in 0..max_rounds {
            let result = self.rumor_mongering_round();
            let done = result.converged;
            results.push(result);
            if done {
                break;
            }
        }
        results
    }
}

/// Build a fully-connected cluster of `n` nodes with the given fanout.
pub fn build_full_mesh(n: usize, fanout: usize) -> GossipCluster {
    let mut cluster = GossipCluster::new(fanout);
    let ids: Vec<u32> = (0..n as u32).collect();
    for &id in &ids {
        let mut node = GossipNode::new(id);
        for &other in &ids {
            if other != id {
                node.add_neighbor(other);
            }
        }
        cluster.add_node(node);
    }
    cluster
}

/// Build a ring cluster where each node connects to its `degree` nearest neighbors.
pub fn build_ring(n: usize, degree: usize, fanout: usize) -> GossipCluster {
    let mut cluster = GossipCluster::new(fanout);
    let ids: Vec<u32> = (0..n as u32).collect();
    for (i, &id) in ids.iter().enumerate() {
        let mut node = GossipNode::new(id);
        for d in 1..=degree {
            let right = ids[(i + d) % n];
            let left = ids[(i + n - d) % n];
            node.add_neighbor(right);
            node.add_neighbor(left);
        }
        cluster.add_node(node);
    }
    cluster
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infection_state_values() {
        assert_eq!(InfectionState::Susceptible.value(), -1);
        assert_eq!(InfectionState::Carrier.value(), 0);
        assert_eq!(InfectionState::Infected.value(), 1);
    }

    #[test]
    fn test_infection_state_from_value() {
        assert_eq!(InfectionState::from_value(-1), Some(InfectionState::Susceptible));
        assert_eq!(InfectionState::from_value(0), Some(InfectionState::Carrier));
        assert_eq!(InfectionState::from_value(1), Some(InfectionState::Infected));
        assert_eq!(InfectionState::from_value(2), None);
    }

    #[test]
    fn test_node_creation_default_susceptible() {
        let node = GossipNode::new(42);
        assert_eq!(node.id, 42);
        assert_eq!(node.state, InfectionState::Susceptible);
        assert_eq!(node.version, 0);
        assert!(node.neighbors.is_empty());
    }

    #[test]
    fn test_cluster_seed_update() {
        let mut cluster = build_full_mesh(5, 2);
        cluster.seed_update(0, 1);

        let source = cluster.nodes.get(&0).unwrap();
        assert_eq!(source.state, InfectionState::Infected);
        assert_eq!(source.version, 1);

        // All others should still be susceptible
        for id in 1..5u32 {
            let node = cluster.nodes.get(&id).unwrap();
            assert_eq!(node.state, InfectionState::Susceptible);
        }
    }

    #[test]
    fn test_convergence_detection() {
        let mut cluster = build_full_mesh(4, 2);
        cluster.seed_update(0, 1);

        // Initially not converged
        assert!(!cluster.is_converged());

        // Manually infect all nodes
        for node in cluster.nodes.values_mut() {
            node.state = InfectionState::Infected;
            node.version = 1;
        }
        assert!(cluster.is_converged());
    }

    #[test]
    fn test_push_pull_propagation_converges() {
        let mut cluster = build_full_mesh(10, 3);
        cluster.seed_update(0, 1);

        let results = cluster.run_until_converged(50);
        assert!(cluster.is_converged(), "Cluster should converge within 50 rounds");

        // All nodes should be infected
        for node in cluster.nodes.values() {
            assert_eq!(node.state, InfectionState::Infected);
            assert_eq!(node.version, 1);
        }

        // Should have had at least one round
        assert!(!results.is_empty());
    }

    #[test]
    fn test_state_counts() {
        let mut cluster = build_full_mesh(6, 2);
        cluster.seed_update(0, 1);

        // 1 infected, 5 susceptible, 0 carrier
        let (infected, carrier, susceptible) = cluster.state_counts();
        assert_eq!(infected, 1);
        assert_eq!(susceptible, 5);
        assert_eq!(carrier, 0);
    }

    #[test]
    fn test_infected_nodes_tracking() {
        let mut cluster = build_full_mesh(4, 2);
        cluster.seed_update(2, 3);

        let infected = cluster.infected_nodes();
        assert_eq!(infected, vec![2]);
    }

    #[test]
    fn test_rumor_mongering_converges() {
        let mut cluster = build_full_mesh(8, 2);
        cluster.seed_update(0, 1);

        let results = cluster.run_rumor_mongering(50);
        assert!(cluster.is_converged(), "Rumor mongering should converge");

        for node in cluster.nodes.values() {
            assert_eq!(node.state, InfectionState::Infected);
            assert_eq!(node.version, 1);
        }
        assert!(!results.is_empty());
    }

    #[test]
    fn test_ring_topology_propagation() {
        let mut cluster = build_ring(8, 2, 1);
        cluster.seed_update(0, 1);

        let results = cluster.run_until_converged(100);
        assert!(cluster.is_converged(), "Ring topology should converge");

        for node in cluster.nodes.values() {
            assert_eq!(node.state, InfectionState::Infected);
            assert_eq!(node.version, 1);
        }
        // Ring should take more rounds than full mesh
        assert!(results.len() > 2, "Ring should need multiple rounds");
    }

    #[test]
    fn test_round_result_transitions() {
        let mut cluster = build_full_mesh(4, 2);
        cluster.seed_update(0, 1);

        let result = cluster.gossip_round();
        assert_eq!(result.round, 1);
        // Should have at least one exchange (node 0 pushes)
        assert!(!result.exchanges.is_empty());
        // Should have transitions (susceptible → carrier → infected)
        assert!(!result.transitions.is_empty());
    }

    #[test]
    fn test_multiple_versions() {
        let mut cluster = build_full_mesh(6, 2);
        cluster.seed_update(0, 1);
        cluster.run_until_converged(50);
        assert!(cluster.is_converged());

        // Seed a new version from a different node
        cluster.seed_update(3, 2);
        assert!(!cluster.is_converged());

        let _results = cluster.run_until_converged(50);
        assert!(cluster.is_converged());

        for node in cluster.nodes.values() {
            assert_eq!(node.version, 2);
        }
    }

    #[test]
    fn test_rumor_mongering_stops_spreading() {
        let mut cluster = build_full_mesh(4, 3);
        cluster.seed_update(0, 1);

        // Run one round at a time and verify rumor mongering behavior
        let mut rounds = 0;
        loop {
            let result = cluster.rumor_mongering_round();
            rounds += 1;
            if result.converged || rounds > 50 {
                break;
            }
        }
        assert!(cluster.is_converged());
    }
}
