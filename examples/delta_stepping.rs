use petgraph::Graph;
use petgraph_paralgs::algo::delta_stepping;

fn main() {
    let mut graph = Graph::<_, u32>::new();

    let a = graph.add_node(());
    let b = graph.add_node(());
    let c = graph.add_node(());
    let d = graph.add_node(());
    let e = graph.add_node(());
    let f = graph.add_node(());

    graph.extend_with_edges([
        (a, b, 2),
        (a, d, 4),
        (b, c, 1),
        (b, f, 7),
        (c, e, 5),
        (e, f, 1),
        (d, e, 1),
    ]);

    // Graph represented with the weight of each edge
    // Edges with '*' are part of the optimal path.
    //
    //     2       1
    // a ----- b ----- c
    // | 4*    | 7     |
    // d       f       | 5
    // | 1*    | 1*    |
    // \------ e ------/

    let path = delta_stepping(&graph, a, |finish| finish == f, |e| *e.weight(), 2);
    assert_eq!(path, Some((6, vec![a, d, e, f])));
}
