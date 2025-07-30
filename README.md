# petgraph-paralgs

![crates.io license](https://img.shields.io/crates/l/petgraph-paralgs)
![crates.io version](https://img.shields.io/crates/v/petgraph-paralgs)
![deps.rs crate dependencies (latest)](https://img.shields.io/deps-rs/petgraph-paralgs/latest)
![GitHub commits since latest release](https://img.shields.io/github/commits-since/beatussum/petgraph-paralgs/latest)
![GitHub last commit](https://img.shields.io/github/last-commit/beatussum/petgraph-paralgs)
![GitHub release date](https://img.shields.io/github/release-date/beatussum/petgraph-paralgs)

**petgraph-paralgs** provides some parallel graph algorithms using the API of the Rust [petgraph library](https://crates.io/crates/petgraph), and powered by [rayon](https://crates.io/crates/rayon).

This project was inspired by the [graphalgs crate](https://crates.io/crates/graphalgs).

## Example

```rust
use petgraph::Graph;
use petgraph_paralgs::algo::delta_stepping;

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
```

## Adding this crate to your project

First, you need to have a Rust toolchain installed.
You can follow the instructions at [this page](https://www.rust-lang.org/learn/get-started).
If you are a **GNU/Linux** user, it should be included in the official repositories of your favorite distribution.
This project is based on the [Cargo](https://doc.rust-lang.org/cargo/) package manager.

```bash
cargo add petgraph-paralgs
```

## Contributing

If you want to contribute to this project, please see [this file](/CONTRIBUTING.md) first.

## Licenses

As explained above, the code of this software is licensed under GPL-3 or any later version.
Details of the rights applying to the various third-party files are described in the [`copyright`](copyright) file in [the Debian `debian/copyright` file format](https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/).
