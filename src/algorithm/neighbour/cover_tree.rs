//! # Cover Tree
//!
//! The Cover Tree data structure is specifically designed to facilitate the speed-up of a nearest neighbor search, see [KNN algorithms](../index.html).
//!
//! ```
//! use smartcore::algorithm::neighbour::cover_tree::*;
//! use smartcore::metrics::distance::Distance;
//!
//! #[derive(Clone)]
//! struct SimpleDistance {} // Our distance function
//!
//! impl Distance<i32> for SimpleDistance {
//!   fn distance(&self, a: &i32, b: &i32) -> f64 { // simple simmetrical scalar distance
//!     (a - b).abs() as f64
//!   }
//! }
//!
//! let data = vec![1, 2, 3, 4, 5, 6, 7, 8, 9]; // data points
//!
//! let mut tree = CoverTree::new(data, SimpleDistance {}).unwrap();
//!
//! tree.find(&5, 3); // find 3 knn points from 5
//!
//! ```
use std::fmt::Debug;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::algorithm::sort::heap_select::HeapSelection;
use crate::error::{Failed, FailedError};
use crate::metrics::distance::Distance;

/// Implements Cover Tree algorithm
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug)]
pub struct CoverTree<T, D: Distance<T>> {
    base: f64,
    inv_log_base: f64,
    distance: D,
    root: Node,
    data: Vec<T>,
    identical_excluded: bool,
}

impl<T, D: Distance<T>> PartialEq for CoverTree<T, D> {
    fn eq(&self, other: &Self) -> bool {
        if self.data.len() != other.data.len() {
            return false;
        }
        for i in 0..self.data.len() {
            if self.distance.distance(&self.data[i], &other.data[i]) != 0f64 {
                return false;
            }
        }
        true
    }
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug)]
struct Node {
    idx: usize,
    max_dist: f64,
    parent_dist: f64,
    children: Vec<Node>,
    _scale: i64,
}

#[derive(Debug)]
struct DistanceSet {
    idx: usize,
    dist: Vec<f64>,
}

impl<T: Debug + PartialEq, D: Distance<T>> CoverTree<T, D> {
    /// Construct a cover tree.
    /// * `data` - vector of data points to search for.
    /// * `distance` - distance metric to use for searching. This function should extend [`Distance`](../../../math/distance/index.html) interface.
    pub fn new(data: Vec<T>, distance: D) -> Result<CoverTree<T, D>, Failed> {
        let base = 1.3f64;
        let root = Node {
            idx: 0,
            max_dist: 0f64,
            parent_dist: 0f64,
            children: Vec::new(),
            _scale: 0,
        };
        let mut tree = CoverTree {
            base,
            inv_log_base: 1f64 / base.ln(),
            distance,
            root,
            data,
            identical_excluded: false,
        };

        tree.build_cover_tree();

        Ok(tree)
    }

    /// Find k nearest neighbors of `p`
    /// * `p` - look for k nearest points to `p`
    /// * `k` - the number of nearest neighbors to return
    pub fn find(&self, p: &T, k: usize) -> Result<Vec<(usize, f64, &T)>, Failed> {
        if k == 0 {
            return Err(Failed::because(FailedError::FindFailed, "k should be > 0"));
        }

        if k > self.data.len() {
            return Err(Failed::because(
                FailedError::FindFailed,
                "k is > than the dataset size",
            ));
        }

        let e = self.get_data_value(self.root.idx);
        let mut d = self.distance.distance(e, p);

        let mut current_cover_set: Vec<(f64, &Node)> = Vec::new();
        let mut zero_set: Vec<(f64, &Node)> = Vec::new();

        current_cover_set.push((d, &self.root));

        let mut heap = HeapSelection::with_capacity(k);
        heap.add(f64::MAX);

        let mut empty_heap = true;
        if !self.identical_excluded || self.get_data_value(self.root.idx) != p {
            heap.add(d);
            empty_heap = false;
        }

        while !current_cover_set.is_empty() {
            let mut next_cover_set: Vec<(f64, &Node)> = Vec::new();
            for par in current_cover_set {
                let parent = par.1;
                for c in 0..parent.children.len() {
                    let child = &parent.children[c];
                    if c == 0 {
                        d = par.0;
                    } else {
                        d = self.distance.distance(self.get_data_value(child.idx), p);
                    }

                    let upper_bound = if empty_heap {
                        f64::INFINITY
                    } else {
                        *heap.peek()
                    };
                    if d <= (upper_bound + child.max_dist) {
                        if c > 0
                            && d < upper_bound
                            && (!self.identical_excluded || self.get_data_value(child.idx) != p)
                        {
                            heap.add(d);
                        }

                        if !child.children.is_empty() {
                            next_cover_set.push((d, child));
                        } else if d <= upper_bound {
                            zero_set.push((d, child));
                        }
                    }
                }
            }
            current_cover_set = next_cover_set;
        }

        let mut neighbors: Vec<(usize, f64, &T)> = Vec::new();
        let upper_bound = *heap.peek();
        for ds in zero_set {
            if ds.0 <= upper_bound {
                let v = self.get_data_value(ds.1.idx);
                if !self.identical_excluded || v != p {
                    neighbors.push((ds.1.idx, ds.0, v));
                }
            }
        }

        if neighbors.len() > k {
            neighbors.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        }
        Ok(neighbors.into_iter().take(k).collect())
    }

    /// Find all nearest neighbors within radius `radius` from `p`
    /// * `p` - look for k nearest points to `p`
    /// * `radius` - radius of the search
    pub fn find_radius(&self, p: &T, radius: f64) -> Result<Vec<(usize, f64, &T)>, Failed> {
        if radius <= 0f64 {
            return Err(Failed::because(
                FailedError::FindFailed,
                "radius should be > 0",
            ));
        }

        let mut neighbors: Vec<(usize, f64, &T)> = Vec::new();

        let mut current_cover_set: Vec<(f64, &Node)> = Vec::new();
        let mut zero_set: Vec<(f64, &Node)> = Vec::new();

        let e = self.get_data_value(self.root.idx);
        let mut d = self.distance.distance(e, p);
        current_cover_set.push((d, &self.root));

        while !current_cover_set.is_empty() {
            let mut next_cover_set: Vec<(f64, &Node)> = Vec::new();
            for par in current_cover_set {
                let parent = par.1;
                for c in 0..parent.children.len() {
                    let child = &parent.children[c];
                    if c == 0 {
                        d = par.0;
                    } else {
                        d = self.distance.distance(self.get_data_value(child.idx), p);
                    }

                    if d <= radius + child.max_dist {
                        if !child.children.is_empty() {
                            next_cover_set.push((d, child));
                        } else if d <= radius {
                            zero_set.push((d, child));
                        }
                    }
                }
            }
            current_cover_set = next_cover_set;
        }

        for ds in zero_set {
            let v = self.get_data_value(ds.1.idx);
            if !self.identical_excluded || v != p {
                neighbors.push((ds.1.idx, ds.0, v));
            }
        }

        Ok(neighbors)
    }

    fn new_leaf(&self, idx: usize) -> Node {
        Node {
            idx,
            max_dist: 0f64,
            parent_dist: 0f64,
            children: Vec::new(),
            _scale: 100,
        }
    }

    fn build_cover_tree(&mut self) {
        let mut point_set: Vec<DistanceSet> = Vec::new();
        let mut consumed_set: Vec<DistanceSet> = Vec::new();

        let point = &self.data[0];
        let idx = 0;
        let mut max_dist = -1f64;

        for i in 1..self.data.len() {
            let dist = self.distance.distance(point, &self.data[i]);
            let set = DistanceSet {
                idx: i,
                dist: vec![dist],
            };
            point_set.push(set);
            if dist > max_dist {
                max_dist = dist;
            }
        }

        self.root = self.batch_insert(
            idx,
            self.get_scale(max_dist),
            self.get_scale(max_dist),
            &mut point_set,
            &mut consumed_set,
        );
    }

    fn batch_insert(
        &self,
        p: usize,
        max_scale: i64,
        top_scale: i64,
        point_set: &mut Vec<DistanceSet>,
        consumed_set: &mut Vec<DistanceSet>,
    ) -> Node {
        if point_set.is_empty() {
            self.new_leaf(p)
        } else {
            let max_dist = self.max(point_set);
            let next_scale = (max_scale - 1).min(self.get_scale(max_dist));
            if next_scale == i64::MIN {
                let mut children: Vec<Node> = Vec::new();
                let mut leaf = self.new_leaf(p);
                children.push(leaf);
                while !point_set.is_empty() {
                    let set = point_set.remove(point_set.len() - 1);
                    leaf = self.new_leaf(set.idx);
                    children.push(leaf);
                    consumed_set.push(set);
                }
                Node {
                    idx: p,
                    max_dist: 0f64,
                    parent_dist: 0f64,
                    children,
                    _scale: 100,
                }
            } else {
                let mut far: Vec<DistanceSet> = Vec::new();
                self.split(point_set, &mut far, max_scale);

                let child = self.batch_insert(p, next_scale, top_scale, point_set, consumed_set);

                if point_set.is_empty() {
                    point_set.append(&mut far);
                    child
                } else {
                    let mut children: Vec<Node> = vec![child];
                    let mut new_point_set: Vec<DistanceSet> = Vec::new();
                    let mut new_consumed_set: Vec<DistanceSet> = Vec::new();

                    while !point_set.is_empty() {
                        let set: DistanceSet = point_set.remove(point_set.len() - 1);

                        let new_dist = set.dist[set.dist.len() - 1];

                        self.dist_split(
                            point_set,
                            &mut new_point_set,
                            self.get_data_value(set.idx),
                            max_scale,
                        );
                        self.dist_split(
                            &mut far,
                            &mut new_point_set,
                            self.get_data_value(set.idx),
                            max_scale,
                        );

                        let mut new_child = self.batch_insert(
                            set.idx,
                            next_scale,
                            top_scale,
                            &mut new_point_set,
                            &mut new_consumed_set,
                        );
                        new_child.parent_dist = new_dist;

                        consumed_set.push(set);
                        children.push(new_child);

                        let fmax = self.get_cover_radius(max_scale);
                        for mut set in new_point_set.drain(0..) {
                            set.dist.remove(set.dist.len() - 1);
                            if set.dist[set.dist.len() - 1] <= fmax {
                                point_set.push(set);
                            } else {
                                far.push(set);
                            }
                        }

                        for mut set in new_consumed_set.drain(0..) {
                            set.dist.remove(set.dist.len() - 1);
                            consumed_set.push(set);
                        }
                    }

                    point_set.append(&mut far);

                    Node {
                        idx: p,
                        max_dist: self.max(consumed_set),
                        parent_dist: 0f64,
                        children,
                        _scale: (top_scale - max_scale),
                    }
                }
            }
        }
    }

    fn split(
        &self,
        point_set: &mut Vec<DistanceSet>,
        far_set: &mut Vec<DistanceSet>,
        max_scale: i64,
    ) {
        let fmax = self.get_cover_radius(max_scale);
        let mut new_set: Vec<DistanceSet> = Vec::new();
        for n in point_set.drain(0..) {
            if n.dist[n.dist.len() - 1] <= fmax {
                new_set.push(n);
            } else {
                far_set.push(n);
            }
        }

        point_set.append(&mut new_set);
    }

    fn dist_split(
        &self,
        point_set: &mut Vec<DistanceSet>,
        new_point_set: &mut Vec<DistanceSet>,
        new_point: &T,
        max_scale: i64,
    ) {
        let fmax = self.get_cover_radius(max_scale);
        let mut new_set: Vec<DistanceSet> = Vec::new();
        for mut n in point_set.drain(0..) {
            let new_dist = self
                .distance
                .distance(new_point, self.get_data_value(n.idx));
            if new_dist <= fmax {
                n.dist.push(new_dist);
                new_point_set.push(n);
            } else {
                new_set.push(n);
            }
        }

        point_set.append(&mut new_set);
    }

    fn get_cover_radius(&self, s: i64) -> f64 {
        self.base.powf(s as f64)
    }

    fn get_data_value(&self, idx: usize) -> &T {
        &self.data[idx]
    }

    fn get_scale(&self, d: f64) -> i64 {
        if d == 0f64 {
            i64::MIN
        } else {
            (self.inv_log_base * d.ln()).ceil() as i64
        }
    }

    fn max(&self, distance_set: &[DistanceSet]) -> f64 {
        let mut max = 0f64;
        for n in distance_set {
            if max < n.dist[n.dist.len() - 1] {
                max = n.dist[n.dist.len() - 1];
            }
        }
        max
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::metrics::distance::Distances;

    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    #[derive(Debug, Clone)]
    struct SimpleDistance {}

    impl Distance<i32> for SimpleDistance {
        fn distance(&self, a: &i32, b: &i32) -> f64 {
            (a - b).abs() as f64
        }
    }

    #[cfg_attr(
        all(target_arch = "wasm32", not(target_os = "wasi")),
        wasm_bindgen_test::wasm_bindgen_test
    )]
    #[test]
    fn cover_tree_test() {
        let data = vec![1, 2, 3, 4, 5, 6, 7, 8, 9];

        let tree = CoverTree::new(data, SimpleDistance {}).unwrap();

        let mut knn = tree.find(&5, 3).unwrap();
        knn.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let knn: Vec<usize> = knn.iter().map(|v| v.0).collect();
        assert_eq!(vec!(3, 4, 5), knn);

        let mut knn = tree.find_radius(&5, 2.0).unwrap();
        knn.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let knn: Vec<i32> = knn.iter().map(|v| *v.2).collect();
        assert_eq!(vec!(3, 4, 5, 6, 7), knn);
    }
    #[cfg_attr(
        all(target_arch = "wasm32", not(target_os = "wasi")),
        wasm_bindgen_test::wasm_bindgen_test
    )]
    #[test]
    fn cover_tree_test1() {
        let data = vec![
            vec![1., 2.],
            vec![3., 4.],
            vec![5., 6.],
            vec![7., 8.],
            vec![9., 10.],
        ];

        let tree = CoverTree::new(data, Distances::euclidian()).unwrap();

        let mut knn = tree.find(&vec![1., 2.], 3).unwrap();
        knn.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let knn: Vec<usize> = knn.iter().map(|v| v.0).collect();

        assert_eq!(vec!(0, 1, 2), knn);
    }
    #[cfg_attr(
        all(target_arch = "wasm32", not(target_os = "wasi")),
        wasm_bindgen_test::wasm_bindgen_test
    )]
    #[test]
    #[cfg(feature = "serde")]
    fn serde() {
        let data = vec![1, 2, 3, 4, 5, 6, 7, 8, 9];

        let tree = CoverTree::new(data, SimpleDistance {}).unwrap();

        let deserialized_tree: CoverTree<i32, SimpleDistance> =
            serde_json::from_str(&serde_json::to_string(&tree).unwrap()).unwrap();

        assert_eq!(tree, deserialized_tree);
    }
}
