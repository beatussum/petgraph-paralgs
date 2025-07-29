use petgraph::{
    algo::{Measure, PositiveMeasure},
    visit::{EdgeRef, IntoEdges, Visitable},
};

use rayon::prelude::*;
use std::{cmp::min_by_key, collections::LinkedList, fmt::Debug, hash::Hash, mem::swap, ops::Div};

type HashMap<K, V> = dashmap::DashMap<K, V, ahash::RandomState>;
type HashMultiMap<K, V> = HashMap<K, Vec<V>>;

fn explore<G, F, K, IsGoal>(
    graph: &G,
    node: G::NodeId,
    buckets: &HashMultiMap<K, G::NodeId>,
    dists: &HashMap<G::NodeId, Distance<K, G::NodeId>>,
    delta: K,
    is_goal: &IsGoal,
    edge_cost: &F,
) -> Explored<K, G::NodeId>
where
    G: IntoEdges + Visitable,
    IsGoal: Fn(G::NodeId) -> bool,
    G::NodeId: Eq + Hash,
    F: Fn(G::EdgeRef) -> K,
    K: Copy + Div<Output = K> + Eq + Hash + PositiveMeasure,
{
    use Explored::*;

    let all @ Distance {
        distance: base_dist,
        ..
    } = dists.get(&node).as_deref().copied().unwrap_or_default();

    if is_goal(node) {
        Solved((node, all))
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

            if cost > delta {
                heavy_edges.push((target, new_dist));
            } else {
                relax(target, new_dist, buckets, dists, delta);
            }
        }

        Unsolved(heavy_edges)
    }
}

fn relax<NodeId, K>(
    node: NodeId,

    new_all @ Distance {
        distance: new_dist, ..
    }: Distance<K, NodeId>,

    buckets: &HashMultiMap<K, NodeId>,
    dists: &HashMap<NodeId, Distance<K, NodeId>>,
    delta: K,
) where
    NodeId: Copy + Eq + Hash,
    K: Copy + Div<Output = K> + Eq + Hash + Measure,
{
    let to_insert = dists.get(&node).as_deref().copied().map_or(
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
        dists.insert(node, new_all);
        buckets.entry(new_dist / delta).or_default().push(node);
    }
}

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

    let buckets = HashMultiMap::from_iter([(K::zero(), vec![start])]);

    let dists = HashMap::from_iter([(
        start,
        Distance {
            distance: K::zero(),
            previous: None,
        },
    )]);

    while let Some(first_index) = { buckets.iter().map(|r| *r.key()).min() } {
        let mut explored_list = ExploredList::default();

        while let Some((_, first_bucket)) = buckets.remove(&first_index) {
            let mut to_append = first_bucket
                .into_par_iter()
                .fold(ExploredList::default, |mut list, node| {
                    let to_push =
                        explore(&graph, node, &buckets, &dists, delta, &is_goal, &edge_cost);

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
            Solved((node, Distance { distance, .. })) => {
                let mut path = Vec::default();

                for node in PathIterator::new(node, &dists) {
                    path.push(node);
                }

                path.reverse();
                return Some((distance, path));
            }

            Unsolved(heavy_edges) => {
                heavy_edges
                    .into_iter()
                    .flatten()
                    .for_each(|(node, new_dist)| relax(node, new_dist, &buckets, &dists, delta));
            }
        }
    }

    None
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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

#[derive(Debug)]
enum Explored<K, NodeId> {
    Solved((NodeId, Distance<K, NodeId>)),
    Unsolved(Vec<(NodeId, Distance<K, NodeId>)>),
}

#[derive(Debug)]
enum ExploredList<K, NodeId> {
    Solved((NodeId, Distance<K, NodeId>)),
    Unsolved(LinkedList<Vec<(NodeId, Distance<K, NodeId>)>>),
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
                    *s = min_by_key(*s, *other, |&(_, Distance { distance, .. })| distance);
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
                    *s = min_by_key(*s, value, |&(_, Distance { distance, .. })| distance);
                }
            }

            Self::Unsolved(s) => match value {
                Explored::Solved(value) => *self = Self::Solved(value),
                Explored::Unsolved(value) => s.push_back(value),
            },
        }
    }
}

impl<W, N> Default for ExploredList<W, N> {
    fn default() -> Self {
        Self::Unsolved(LinkedList::default())
    }
}

struct PathIterator<'a, K, NodeId> {
    current: Option<NodeId>,
    dists: &'a HashMap<NodeId, Distance<K, NodeId>>,
}

impl<'a, K, NodeId> PathIterator<'a, K, NodeId> {
    pub fn new(current: NodeId, dists: &'a HashMap<NodeId, Distance<K, NodeId>>) -> Self {
        Self {
            current: Some(current),
            dists,
        }
    }
}

impl<K, NodeId> Iterator for PathIterator<'_, K, NodeId>
where
    K: Copy,
    NodeId: Copy + Eq + Hash,
{
    type Item = NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.current;

        let next = self
            .current
            .as_ref()
            .and_then(|current| self.dists.get(current))
            .as_deref()
            .copied()
            .and_then(|Distance { previous, .. }| previous);

        self.current = next;
        current
    }
}
