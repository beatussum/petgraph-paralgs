//! Implementation of [`delta_stepping`]

use petgraph::{
    algo::{Measure, PositiveMeasure},
    visit::{EdgeRef, GraphBase, IntoEdges, Visitable},
};

use rayon::prelude::*;

use std::{
    cmp::{Ordering, min_by_key},
    collections::LinkedList,
    fmt::Debug,
    hash::Hash,
    mem::{replace, swap},
    ops::Div,
};

type HashMap<K, V> = dashmap::DashMap<K, V, ahash::RandomState>;
type HashMultiMap<K, V> = HashMap<K, Vec<V>>;

#[derive(Clone, Copy, Debug)]
struct Distance<K, NodeId> {
    distance: K,
    previous: Option<NodeId>,
}

impl<K: PositiveMeasure, NodeId> Default for Distance<K, NodeId> {
    fn default() -> Self {
        Self {
            distance: K::zero(),
            previous: None,
        }
    }
}

impl<K: PartialEq, NodeId> PartialEq for Distance<K, NodeId> {
    fn eq(&self, other: &Self) -> bool {
        self.distance.eq(&other.distance)
    }
}

impl<K: Eq, NodeId> Eq for Distance<K, NodeId> {}

impl<K: PartialOrd, NodeId> PartialOrd for Distance<K, NodeId> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.distance.partial_cmp(&other.distance)
    }
}

impl<K: Ord, NodeId> Ord for Distance<K, NodeId> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.distance.cmp(&other.distance)
    }
}

#[derive(Clone, Copy, Debug)]
struct DistancedNode<K, NodeId> {
    node: NodeId,
    distance: Distance<K, NodeId>,
}

#[derive(Debug)]
enum Explored<K, NodeId> {
    Solved(DistancedNode<K, NodeId>),
    Unsolved(Vec<DistancedNode<K, NodeId>>),
}

#[derive(Debug)]
enum ExploredList<K, NodeId> {
    Solved(DistancedNode<K, NodeId>),
    Unsolved(LinkedList<Vec<DistancedNode<K, NodeId>>>),
}

impl<K, NodeId> ExploredList<K, NodeId>
where
    NodeId: Copy,
    K: Copy + Measure + Ord,
{
    pub fn append(&mut self, other: &mut Self) {
        use ExploredList::*;

        match self {
            Solved(s) => {
                if let Solved(other) = other {
                    *s = min_by_key(*s, *other, |&DistancedNode { distance, .. }| distance);
                }
            }

            Unsolved(s) => match other {
                Solved(_) => swap(self, other),
                Unsolved(other) => s.append(other),
            },
        }
    }

    pub fn push(&mut self, value: Explored<K, NodeId>) {
        match self {
            Self::Solved(s) => {
                if let Explored::Solved(value) = value {
                    *s = min_by_key(*s, value, |&DistancedNode { distance, .. }| distance);
                }
            }

            Self::Unsolved(s) => match value {
                Explored::Solved(value) => *self = Self::Solved(value),
                Explored::Unsolved(value) => s.push_back(value),
            },
        }
    }
}

impl<K, NodeId> Default for ExploredList<K, NodeId> {
    fn default() -> Self {
        Self::Unsolved(LinkedList::default())
    }
}

struct PathIterator<K, NodeId> {
    current: Option<NodeId>,
    dists: HashMap<NodeId, Distance<K, NodeId>>,
}

impl<K, NodeId> PathIterator<K, NodeId> {
    pub fn new(start: NodeId, dists: HashMap<NodeId, Distance<K, NodeId>>) -> Self {
        Self {
            current: Some(start),
            dists,
        }
    }
}

impl<K, NodeId> Iterator for PathIterator<K, NodeId>
where
    K: Copy,
    NodeId: Copy + Eq + Hash,
{
    type Item = NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self
            .current
            .as_ref()
            .and_then(|current| self.dists.get(current))
            .as_deref()
            .copied()
            .and_then(|Distance { previous, .. }| previous);

        replace(&mut self.current, next)
    }
}

struct NodeManager<K, NodeId> {
    buckets: HashMultiMap<K, NodeId>,
    dists: HashMap<NodeId, Distance<K, NodeId>>,
    delta: K,
}

impl<K, NodeId> NodeManager<K, NodeId>
where
    K: Copy + Div<Output = K> + Eq + Hash + Ord + PositiveMeasure,
    NodeId: Copy + Eq + Hash,
{
    pub fn new(delta: K, start: NodeId) -> Self {
        let buckets = HashMultiMap::from_iter([(K::zero(), vec![start])]);

        let dists = HashMap::from_iter([(
            start,
            Distance {
                distance: K::zero(),
                previous: None,
            },
        )]);

        Self {
            buckets,
            dists,
            delta,
        }
    }

    pub fn first_bucket_index(&self) -> Option<K> {
        self.buckets.iter().map(|r| *r.key()).min()
    }

    pub fn path(self, start: NodeId) -> PathIterator<K, NodeId> {
        PathIterator::new(start, self.dists)
    }

    pub fn explore<G, F, IsGoal>(
        &self,
        graph: &G,
        node: NodeId,
        is_goal: &IsGoal,
        edge_cost: &F,
    ) -> Explored<K, NodeId>
    where
        G: GraphBase<NodeId = NodeId> + IntoEdges + Visitable,
        IsGoal: Fn(G::NodeId) -> bool,
        F: Fn(G::EdgeRef) -> K,
    {
        use Explored::*;

        let distance @ Distance {
            distance: base_dist,
            ..
        } = self
            .dists
            .get(&node)
            .as_deref()
            .copied()
            .unwrap_or_default();

        if is_goal(node) {
            Solved(DistancedNode { node, distance })
        } else {
            let mut heavy_edges = Vec::default();
            let previous = Some(node);

            for (cost, target) in graph
                .edges(node)
                .map(|edge| (edge_cost(edge), edge.target()))
            {
                let new_dist = Distance {
                    distance: base_dist + cost,
                    previous,
                };

                if cost > self.delta {
                    heavy_edges.push(DistancedNode {
                        node: target,
                        distance: new_dist,
                    });
                } else {
                    self.relax(target, new_dist);
                }
            }

            Unsolved(heavy_edges)
        }
    }

    pub fn relax(
        &self,
        node: NodeId,

        new_all @ Distance {
            distance: new_dist, ..
        }: Distance<K, NodeId>,
    ) {
        let to_insert = self.dists.get(&node).as_deref().copied().map_or(
            Some(new_all),
            |Distance {
                 distance: old_dist, ..
             }| { (new_dist < old_dist).then_some(new_all) },
        );

        if let Some(
            new_all @ Distance {
                distance: new_dist, ..
            },
        ) = to_insert
        {
            self.dists.insert(node, new_all);

            self.buckets
                .entry(new_dist / self.delta)
                .or_default()
                .push(node);
        }
    }

    pub fn remove_bucket(&self, index: &K) -> Option<(K, Vec<NodeId>)> {
        self.buckets.remove(index)
    }
}

/// Delta-Stepping single-source shortest path algorithm
///
/// Computes the shortest path from `start` to `finish`, including the total
/// path cost.
///
/// `finish` is implicitly given by the `is_goal` callback, which should return
/// `true` if the given node is the finish node.
///
/// The function `edge_cost` should return the cost for a particular edge. Edge
/// costs must be **positive**.
///
/// # Arguments
///
/// - `graph` - The weighted graph
/// - `start` - The start node
/// - `is_goal` - The callback that defines the goal node
/// - `edge_cost` - The callback that returns the cost of a particular edge
/// - `delta` - The delta parameter (see [this section](#complexity) for more
///   information)
///
/// # Returns
///
/// - `Some(K, Vec<G::NodeId>)` - The total cost and path from start to finish
///   (if one was found).
/// - `None` - If no path was found.
///
/// # Complexity
///
/// The time complexity largely depends on
/// - the number of threads used (see [`rayon::ThreadPoolBuilder::num_threads`]
///   for more information),
/// - the configuration of the graph (values of the weighted edges),
/// - and the delta parameter.
///
/// The higher the delta parameter, the more the algorithm processes node
/// explorations in parallel, but the more potentially unecessary tasks it
/// processes.
///
/// In the worst case, the algorithm will behave like
/// [`petgraph::algo::dijkstra()`].
///
/// # Example
///
/// You can consult this example at `examples/delta_stepping.rs`.
///
/// ```rust
#[allow(clippy::needless_doctest_main)]
#[doc = include_str!("../../examples/delta_stepping.rs")]
/// ```
pub fn delta_stepping<G, F, K, IsGoal>(
    graph: G,
    start: G::NodeId,
    is_goal: IsGoal,
    edge_cost: F,
    delta: K,
) -> Option<(K, Vec<G::NodeId>)>
where
    G: IntoEdges + Visitable + Sync,
    IsGoal: Fn(G::NodeId) -> bool + Sync,
    G::NodeId: Eq + Hash + Send + Sync,
    F: Fn(G::EdgeRef) -> K + Sync,
    K: Div<Output = K> + Ord + Eq + Hash + PositiveMeasure + Send + Sync,
{
    use ExploredList::*;

    let node_manager = NodeManager::new(delta, start);

    while let Some(first_index) = node_manager.first_bucket_index() {
        let mut explored_list = ExploredList::default();

        while let Some((_, first_bucket)) = node_manager.remove_bucket(&first_index) {
            let mut to_append = first_bucket
                .into_par_iter()
                .fold(ExploredList::default, |mut list, node| {
                    let to_push = node_manager.explore(&graph, node, &is_goal, &edge_cost);
                    list.push(to_push);
                    list
                })
                .reduce(ExploredList::default, |mut lhs, mut rhs| {
                    lhs.append(&mut rhs);
                    lhs
                });

            explored_list.append(&mut to_append);
        }

        match explored_list {
            Solved(DistancedNode {
                node,
                distance: Distance { distance, .. },
            }) => {
                let mut path = Vec::default();

                for node in node_manager.path(node) {
                    path.push(node);
                }

                path.reverse();
                return Some((distance, path));
            }

            Unsolved(heavy_edges) => {
                heavy_edges
                    .into_iter()
                    .flatten()
                    .for_each(|DistancedNode { node, distance }| {
                        node_manager.relax(node, distance)
                    });
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    macro_rules! test_delta {
        () => {
            #[test]
            fn only_heavy_edges() {
                build_with(1);
            }

            #[test]
            fn mixed() {
                build_with(5);
            }

            #[test]
            fn one_bucket() {
                build_with(u8::MAX);
            }
        };
    }

    mod chain {
        use crate::algo::delta_stepping;
        use petgraph::graph::UnGraph;

        fn build_with(delta: u8) {
            let graph = UnGraph::<(), u8>::from_edges([(0, 1, 1), (1, 2, 5), (2, 3, 2), (3, 4, 5)]);

            let (distance, path) = delta_stepping(
                &graph,
                0.into(),
                |node| node == 4.into(),
                |edge| *edge.weight(),
                delta,
            )
            .expect("No solution found");

            let path = path.into_iter().map(|i| i.index()).collect::<Vec<_>>();

            assert_eq!(path, [0, 1, 2, 3, 4]);
            assert_eq!(distance, 13);
        }

        test_delta!();
    }

    mod complete {
        use crate::algo::delta_stepping;
        use petgraph::graph::UnGraph;

        fn build_with(delta: u8) {
            let graph = UnGraph::<(), u8>::from_edges([
                (0, 1, 1),
                (1, 2, 1),
                (2, 3, 1),
                (3, 0, 5),
                (1, 3, 5),
                (0, 2, 5),
            ]);

            let (distance, path) = delta_stepping(
                &graph,
                0.into(),
                |node| node == 3.into(),
                |edge| *edge.weight(),
                delta,
            )
            .expect("No solution found");

            let path = path.into_iter().map(|i| i.index()).collect::<Vec<_>>();

            assert_eq!(path, [0, 1, 2, 3]);
            assert_eq!(distance, 3);
        }

        test_delta!();
    }

    mod empty {
        use crate::algo::delta_stepping;
        use petgraph::graph::UnGraph;

        fn build_with(delta: u8) {
            let graph = UnGraph::<(), u8>::default();

            let found = delta_stepping(
                &graph,
                0.into(),
                |node| node == 3.into(),
                |edge| *edge.weight(),
                delta,
            );

            assert!(found.is_none());
        }

        test_delta!();
    }

    mod greedy_is_not_solution {
        use crate::algo::delta_stepping;
        use petgraph::graph::DiGraph;

        fn build_with(delta: u8) {
            let graph = DiGraph::<(), u8>::from_edges([
                (0, 1, 5),
                (1, 2, 1),
                (0, 3, 1),
                (3, 4, 2),
                (4, 5, 5),
                (4, 6, 5),
            ]);

            let (distance, path) = delta_stepping(
                &graph,
                0.into(),
                |node| graph.edges(node).next().is_none(),
                |edge| *edge.weight(),
                delta,
            )
            .expect("No solution found");

            let path = path.into_iter().map(|i| i.index()).collect::<Vec<_>>();

            assert_eq!(path, [0, 1, 2]);
            assert_eq!(distance, 6);
        }

        test_delta!();
    }

    mod none {
        use crate::algo::delta_stepping;
        use petgraph::graph::UnGraph;

        fn build_with(delta: u8) {
            let graph = UnGraph::<(), u8>::from_edges([(0, 1, 1), (1, 2, 5), (3, 4, 5)]);

            let found = delta_stepping(
                &graph,
                0.into(),
                |node| node == 4.into(),
                |edge| *edge.weight(),
                delta,
            );

            assert!(found.is_none());
        }

        test_delta!();
    }

    mod tree {
        use crate::algo::delta_stepping;
        use petgraph::graph::DiGraph;

        fn build_with(delta: u8) {
            let graph = DiGraph::<(), u8>::from_edges([
                (0, 1, 1),
                (0, 2, 5),
                (1, 3, 5),
                (1, 4, 2),
                (2, 5, 5),
                (2, 6, 2),
                (3, 7, 3),
                (3, 8, 5),
                (4, 9, 3),
                (4, 10, 5),
                (5, 11, 3),
                (5, 12, 5),
                (6, 13, 3),
                (6, 14, 5),
            ]);

            let (distance, path) = delta_stepping(
                &graph,
                0.into(),
                |node| graph.edges(node).next().is_none(),
                |edge| *edge.weight(),
                delta,
            )
            .expect("No solution found");

            let path = path.into_iter().map(|i| i.index()).collect::<Vec<_>>();

            assert_eq!(path, [0, 1, 4, 9]);
            assert_eq!(distance, 6);
        }

        test_delta!();
    }
}
