use std::{collections::BTreeMap, ops::Index, path::PathBuf, rc::Rc};

/**
 * A directed acyclic graph of dependencies.
 *
 * The algorithm is pretty simple:
 * First, to construct the simple directed graph, the root input is walked for its dependents.
 * Next, while walking the directed graph, if any cycles are found, they are moved into a child
 * graph (therefore making the parent and child acyclic).
 *
 * Note that the ID of the root node always has a value of 0.
 *
 * TODO: child graphs and cycle detection
 */
pub struct DependencyDAG {
    /// The nodes in this graph.
    nodes: Vec<Dependency>,
    /// The edges between the nodes.
    /// Edges are directional, from the key to the value(s).
    edges: BTreeMap<DAGNodeId, Vec<DAGNodeId>>,
}

impl DependencyDAG {
    /// Creates a new DAG.
    pub fn new(root: Dependency) -> Self {
        Self {
            nodes: vec![root],
            edges: Default::default(),
        }
    }

    /// Adds a new dependency.
    /// This cannot ever create a cycle because a new node is created for the dependency.
    pub fn add_dependency(&mut self, from: DAGNodeId, dep: Dependency) -> DAGNodeId {
        let new_id = DAGNodeId(self.nodes.len());
        self.nodes.push(dep);
        self.edges.entry(from).or_default().push(new_id);
        new_id
    }

    /// Walk the DAG from the given node.
    /// Nodes may be walked twice if they are dependended on by multiple nodes.
    pub fn walk(&self, f: &mut impl FnMut(&Dependency), node: DAGNodeId) {
        f(&self.nodes[node.0]);
        if let Some(deps) = self.edges.get(&node) {
            for dep in deps {
                f(&self.nodes[dep.0]);
                self.walk(f, *dep);
            }
        }
    }

    /// Destructively walks this DAG, starting from the root node.
    /// Nodes are never walked twice.
    pub fn destructive_walk(mut self, mut f: impl FnMut(Dependency)) {
        let nodes_len = self.nodes.len();
        let mut node_stack = Vec::new();
        let mut new_stack = vec![DAGNodeId::ROOT];
        while node_stack.len() < nodes_len {
            for node in std::mem::replace(&mut new_stack, Vec::new()).into_iter() {
                // Removing ensures that deps are not walked twice
                if let Some(deps) = self.edges.remove(&node) {
                    for dep in deps {
                        if !node_stack.contains(&dep) {
                            new_stack.push(dep);
                        }
                    }
                }
            }
            // node stack <- new stack
            node_stack.extend(new_stack.iter());
        }
        // Transform into Options to allow ordering to be preserved
        let mut nodes = self.nodes.into_iter().map(Some).collect::<Vec<_>>();
        let node_stack = node_stack.into_iter().map(|id| nodes[id.0].take().unwrap());
        for node in node_stack {
            f(node);
        }
    }

    /// Gets the node associated with a given ID.
    pub fn get(&self, id: DAGNodeId) -> &Dependency {
        &self.nodes[id.0]
    }
}

impl Index<DAGNodeId> for DependencyDAG {
    type Output = Dependency;

    fn index(&self, index: DAGNodeId) -> &Self::Output {
        self.get(index)
    }
}

// /// A node in a DAG.
// pub enum DAGNode {
//     /// A leaf - dependency.
//     Dependency(Dependency),
//     /// A child DAG.
//     ChildGraph(DependencyDAG),
// }

/// An ID that referes to a DAG node. IDs are local to their graph.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct DAGNodeId(usize);

impl DAGNodeId {
    const ROOT: Self = Self(0);
}

/// A dependency.
pub struct Dependency {
    /// The input path of the dependency.
    pub path: Rc<PathBuf>,
    /// The type of dependency.
    pub ty: DependencyType,
}

pub enum DependencyType {
    StyleChunk,
    Page,
    Image {
        /// Whether the image needs to be converted to WebP.
        needs_reprocessing: bool,
    },
}
