//! Dynamic AABB tree (`b2DynamicTree`) and broad phase (`b2BroadPhase`), Box2D 2.3.1.
//! Node pool growth and free-list order match the original because proxy ids order the pairs.

use crate::math::*;
use crate::settings::*;
use crate::shape::{Aabb, RayCastInput, test_overlap_aabb};

pub const NULL_NODE: i32 = -1;

#[derive(Clone, Copy, Debug, Default)]
struct TreeNode {
    aabb: Aabb,
    user_data: usize,
    /// Parent index, or the next free node when the node is pooled.
    parent: i32,
    child1: i32,
    child2: i32,
    /// Leaf = 0, free node = -1.
    height: i32,
}

impl TreeNode {
    fn is_leaf(&self) -> bool {
        self.child1 == NULL_NODE
    }
}

#[derive(Clone, Debug)]
pub struct DynamicTree {
    root: i32,
    nodes: Vec<TreeNode>,
    node_count: i32,
    free_list: i32,
    insertion_count: i32,
}

impl Default for DynamicTree {
    fn default() -> Self {
        Self::new()
    }
}

impl DynamicTree {
    pub fn new() -> Self {
        const CAPACITY: usize = 16;
        let mut nodes = vec![TreeNode::default(); CAPACITY];
        for (i, n) in nodes.iter_mut().enumerate().take(CAPACITY - 1) {
            n.parent = i as i32 + 1;
            n.height = -1;
        }
        nodes[CAPACITY - 1].parent = NULL_NODE;
        nodes[CAPACITY - 1].height = -1;
        DynamicTree {
            root: NULL_NODE,
            nodes,
            node_count: 0,
            free_list: 0,
            insertion_count: 0,
        }
    }

    fn allocate_node(&mut self) -> i32 {
        if self.free_list == NULL_NODE {
            let old_capacity = self.nodes.len();
            let new_capacity = old_capacity * 2;
            self.nodes.resize(new_capacity, TreeNode::default());
            for i in self.node_count as usize..new_capacity - 1 {
                self.nodes[i].parent = i as i32 + 1;
                self.nodes[i].height = -1;
            }
            self.nodes[new_capacity - 1].parent = NULL_NODE;
            self.nodes[new_capacity - 1].height = -1;
            self.free_list = self.node_count;
        }
        let id = self.free_list;
        let n = &mut self.nodes[id as usize];
        self.free_list = n.parent;
        n.parent = NULL_NODE;
        n.child1 = NULL_NODE;
        n.child2 = NULL_NODE;
        n.height = 0;
        n.user_data = 0;
        self.node_count += 1;
        id
    }

    fn free_node(&mut self, id: i32) {
        let n = &mut self.nodes[id as usize];
        n.parent = self.free_list;
        n.height = -1;
        self.free_list = id;
        self.node_count -= 1;
    }

    fn n(&self, i: i32) -> &TreeNode {
        &self.nodes[i as usize]
    }

    fn nm(&mut self, i: i32) -> &mut TreeNode {
        &mut self.nodes[i as usize]
    }

    pub fn create_proxy(&mut self, aabb: &Aabb, user_data: usize) -> i32 {
        let proxy_id = self.allocate_node();
        let r = vec2(AABB_EXTENSION, AABB_EXTENSION);
        let n = self.nm(proxy_id);
        n.aabb.lower_bound = aabb.lower_bound - r;
        n.aabb.upper_bound = aabb.upper_bound + r;
        n.user_data = user_data;
        n.height = 0;
        self.insert_leaf(proxy_id);
        proxy_id
    }

    pub fn destroy_proxy(&mut self, proxy_id: i32) {
        self.remove_leaf(proxy_id);
        self.free_node(proxy_id);
    }

    /// Returns true if the proxy was re-inserted.
    pub fn move_proxy(&mut self, proxy_id: i32, aabb: &Aabb, displacement: Vec2) -> bool {
        if self.n(proxy_id).aabb.contains(aabb) {
            return false;
        }
        self.remove_leaf(proxy_id);

        let mut b = *aabb;
        let r = vec2(AABB_EXTENSION, AABB_EXTENSION);
        b.lower_bound -= r;
        b.upper_bound += r;

        let d = AABB_MULTIPLIER * displacement;
        if d.x < 0.0 {
            b.lower_bound.x += d.x;
        } else {
            b.upper_bound.x += d.x;
        }
        if d.y < 0.0 {
            b.lower_bound.y += d.y;
        } else {
            b.upper_bound.y += d.y;
        }
        self.nm(proxy_id).aabb = b;
        self.insert_leaf(proxy_id);
        true
    }

    pub fn user_data(&self, proxy_id: i32) -> usize {
        self.n(proxy_id).user_data
    }

    pub fn fat_aabb(&self, proxy_id: i32) -> &Aabb {
        &self.n(proxy_id).aabb
    }

    fn insert_leaf(&mut self, leaf: i32) {
        self.insertion_count += 1;
        if self.root == NULL_NODE {
            self.root = leaf;
            self.nm(leaf).parent = NULL_NODE;
            return;
        }

        let leaf_aabb = self.n(leaf).aabb;
        let mut index = self.root;
        while !self.n(index).is_leaf() {
            let child1 = self.n(index).child1;
            let child2 = self.n(index).child2;

            let area = self.n(index).aabb.perimeter();
            let combined_area = Aabb::combined(&self.n(index).aabb, &leaf_aabb).perimeter();
            let cost = 2.0 * combined_area;
            let inheritance_cost = 2.0 * (combined_area - area);

            let descend_cost = |tree: &Self, child: i32| {
                let aabb = Aabb::combined(&leaf_aabb, &tree.n(child).aabb);
                if tree.n(child).is_leaf() {
                    aabb.perimeter() + inheritance_cost
                } else {
                    let old_area = tree.n(child).aabb.perimeter();
                    let new_area = aabb.perimeter();
                    (new_area - old_area) + inheritance_cost
                }
            };
            let cost1 = descend_cost(self, child1);
            let cost2 = descend_cost(self, child2);

            if cost < cost1 && cost < cost2 {
                break;
            }
            index = if cost1 < cost2 { child1 } else { child2 };
        }

        let sibling = index;
        let old_parent = self.n(sibling).parent;
        let new_parent = self.allocate_node();
        let sibling_aabb = self.n(sibling).aabb;
        let sibling_height = self.n(sibling).height;
        {
            let p = self.nm(new_parent);
            p.parent = old_parent;
            p.user_data = 0;
            p.aabb = Aabb::combined(&leaf_aabb, &sibling_aabb);
            p.height = sibling_height + 1;
        }

        if old_parent != NULL_NODE {
            if self.n(old_parent).child1 == sibling {
                self.nm(old_parent).child1 = new_parent;
            } else {
                self.nm(old_parent).child2 = new_parent;
            }
        } else {
            self.root = new_parent;
        }
        self.nm(new_parent).child1 = sibling;
        self.nm(new_parent).child2 = leaf;
        self.nm(sibling).parent = new_parent;
        self.nm(leaf).parent = new_parent;

        let mut index = self.n(leaf).parent;
        while index != NULL_NODE {
            index = self.balance(index);
            let child1 = self.n(index).child1;
            let child2 = self.n(index).child2;
            let height = 1 + self.n(child1).height.max(self.n(child2).height);
            let aabb = Aabb::combined(&self.n(child1).aabb, &self.n(child2).aabb);
            let node = self.nm(index);
            node.height = height;
            node.aabb = aabb;
            index = self.n(index).parent;
        }
    }

    fn remove_leaf(&mut self, leaf: i32) {
        if leaf == self.root {
            self.root = NULL_NODE;
            return;
        }
        let parent = self.n(leaf).parent;
        let grand_parent = self.n(parent).parent;
        let sibling = if self.n(parent).child1 == leaf {
            self.n(parent).child2
        } else {
            self.n(parent).child1
        };

        if grand_parent != NULL_NODE {
            if self.n(grand_parent).child1 == parent {
                self.nm(grand_parent).child1 = sibling;
            } else {
                self.nm(grand_parent).child2 = sibling;
            }
            self.nm(sibling).parent = grand_parent;
            self.free_node(parent);

            let mut index = grand_parent;
            while index != NULL_NODE {
                index = self.balance(index);
                let child1 = self.n(index).child1;
                let child2 = self.n(index).child2;
                let aabb = Aabb::combined(&self.n(child1).aabb, &self.n(child2).aabb);
                let height = 1 + self.n(child1).height.max(self.n(child2).height);
                let node = self.nm(index);
                node.aabb = aabb;
                node.height = height;
                index = self.n(index).parent;
            }
        } else {
            self.root = sibling;
            self.nm(sibling).parent = NULL_NODE;
            self.free_node(parent);
        }
    }

    /// Rotates node `i_a` if it is imbalanced; returns the new subtree root.
    fn balance(&mut self, i_a: i32) -> i32 {
        let a = *self.n(i_a);
        if a.is_leaf() || a.height < 2 {
            return i_a;
        }
        let i_b = a.child1;
        let i_c = a.child2;
        let balance = self.n(i_c).height - self.n(i_b).height;

        if balance > 1 {
            let i_f = self.n(i_c).child1;
            let i_g = self.n(i_c).child2;

            self.nm(i_c).child1 = i_a;
            let a_parent = self.n(i_a).parent;
            self.nm(i_c).parent = a_parent;
            self.nm(i_a).parent = i_c;

            let c_parent = self.n(i_c).parent;
            if c_parent != NULL_NODE {
                if self.n(c_parent).child1 == i_a {
                    self.nm(c_parent).child1 = i_c;
                } else {
                    self.nm(c_parent).child2 = i_c;
                }
            } else {
                self.root = i_c;
            }

            if self.n(i_f).height > self.n(i_g).height {
                self.nm(i_c).child2 = i_f;
                self.nm(i_a).child2 = i_g;
                self.nm(i_g).parent = i_a;
                let a_aabb = Aabb::combined(&self.n(i_b).aabb, &self.n(i_g).aabb);
                self.nm(i_a).aabb = a_aabb;
                let c_aabb = Aabb::combined(&self.n(i_a).aabb, &self.n(i_f).aabb);
                self.nm(i_c).aabb = c_aabb;
                let a_h = 1 + self.n(i_b).height.max(self.n(i_g).height);
                self.nm(i_a).height = a_h;
                let c_h = 1 + self.n(i_a).height.max(self.n(i_f).height);
                self.nm(i_c).height = c_h;
            } else {
                self.nm(i_c).child2 = i_g;
                self.nm(i_a).child2 = i_f;
                self.nm(i_f).parent = i_a;
                let a_aabb = Aabb::combined(&self.n(i_b).aabb, &self.n(i_f).aabb);
                self.nm(i_a).aabb = a_aabb;
                let c_aabb = Aabb::combined(&self.n(i_a).aabb, &self.n(i_g).aabb);
                self.nm(i_c).aabb = c_aabb;
                let a_h = 1 + self.n(i_b).height.max(self.n(i_f).height);
                self.nm(i_a).height = a_h;
                let c_h = 1 + self.n(i_a).height.max(self.n(i_g).height);
                self.nm(i_c).height = c_h;
            }
            return i_c;
        }

        if balance < -1 {
            let i_d = self.n(i_b).child1;
            let i_e = self.n(i_b).child2;

            self.nm(i_b).child1 = i_a;
            let a_parent = self.n(i_a).parent;
            self.nm(i_b).parent = a_parent;
            self.nm(i_a).parent = i_b;

            let b_parent = self.n(i_b).parent;
            if b_parent != NULL_NODE {
                if self.n(b_parent).child1 == i_a {
                    self.nm(b_parent).child1 = i_b;
                } else {
                    self.nm(b_parent).child2 = i_b;
                }
            } else {
                self.root = i_b;
            }

            if self.n(i_d).height > self.n(i_e).height {
                self.nm(i_b).child2 = i_d;
                self.nm(i_a).child1 = i_e;
                self.nm(i_e).parent = i_a;
                let a_aabb = Aabb::combined(&self.n(i_c).aabb, &self.n(i_e).aabb);
                self.nm(i_a).aabb = a_aabb;
                let b_aabb = Aabb::combined(&self.n(i_a).aabb, &self.n(i_d).aabb);
                self.nm(i_b).aabb = b_aabb;
                let a_h = 1 + self.n(i_c).height.max(self.n(i_e).height);
                self.nm(i_a).height = a_h;
                let b_h = 1 + self.n(i_a).height.max(self.n(i_d).height);
                self.nm(i_b).height = b_h;
            } else {
                self.nm(i_b).child2 = i_e;
                self.nm(i_a).child1 = i_d;
                self.nm(i_d).parent = i_a;
                let a_aabb = Aabb::combined(&self.n(i_c).aabb, &self.n(i_d).aabb);
                self.nm(i_a).aabb = a_aabb;
                let b_aabb = Aabb::combined(&self.n(i_a).aabb, &self.n(i_e).aabb);
                self.nm(i_b).aabb = b_aabb;
                let a_h = 1 + self.n(i_c).height.max(self.n(i_d).height);
                self.nm(i_a).height = a_h;
                let b_h = 1 + self.n(i_a).height.max(self.n(i_e).height);
                self.nm(i_b).height = b_h;
            }
            return i_b;
        }
        i_a
    }

    pub fn height(&self) -> i32 {
        if self.root == NULL_NODE {
            0
        } else {
            self.n(self.root).height
        }
    }

    fn compute_height_of(&self, node_id: i32) -> i32 {
        let node = self.n(node_id);
        if node.is_leaf() {
            return 0;
        }
        1 + self
            .compute_height_of(node.child1)
            .max(self.compute_height_of(node.child2))
    }

    /// Structural check used by tests: heights, parent links and fat AABB containment.
    pub fn validate(&self) -> bool {
        fn walk(t: &DynamicTree, i: i32) -> bool {
            if i == NULL_NODE {
                return true;
            }
            let n = t.n(i);
            if n.is_leaf() {
                return n.height == 0;
            }
            let (c1, c2) = (n.child1, n.child2);
            let ok = t.n(c1).parent == i
                && t.n(c2).parent == i
                && n.height == 1 + t.n(c1).height.max(t.n(c2).height)
                && n.aabb == Aabb::combined(&t.n(c1).aabb, &t.n(c2).aabb);
            ok && walk(t, c1) && walk(t, c2)
        }
        let mut free_count = 0;
        let mut free_index = self.free_list;
        while free_index != NULL_NODE {
            free_index = self.n(free_index).parent;
            free_count += 1;
        }
        walk(self, self.root)
            && (self.root == NULL_NODE || self.n(self.root).parent == NULL_NODE)
            && self.height()
                == if self.root == NULL_NODE {
                    0
                } else {
                    self.compute_height_of(self.root)
                }
            && self.node_count + free_count == self.nodes.len() as i32
    }

    /// Calls `callback(proxy_id)` for every leaf overlapping `aabb`; stop when it returns false.
    pub fn query(&self, aabb: &Aabb, mut callback: impl FnMut(i32) -> bool) {
        let mut stack = vec![self.root];
        while let Some(node_id) = stack.pop() {
            if node_id == NULL_NODE {
                continue;
            }
            let node = self.n(node_id);
            if test_overlap_aabb(&node.aabb, aabb) {
                if node.is_leaf() {
                    if !callback(node_id) {
                        return;
                    }
                } else {
                    stack.push(node.child1);
                    stack.push(node.child2);
                }
            }
        }
    }

    /// The callback returns a new max fraction; 0 ends the cast, negative ignores the proxy.
    pub fn ray_cast(
        &self,
        input: &RayCastInput,
        mut callback: impl FnMut(&RayCastInput, i32) -> Real,
    ) {
        let p1 = input.p1;
        let p2 = input.p2;
        let mut r = p2 - p1;
        r.normalize();
        let v = cross_sv(1.0, r);
        let abs_v = abs2(v);
        let mut max_fraction = input.max_fraction;

        let mut t = p1 + max_fraction * (p2 - p1);
        let mut segment_aabb = Aabb {
            lower_bound: min2(p1, t),
            upper_bound: max2(p1, t),
        };

        let mut stack = vec![self.root];
        while let Some(node_id) = stack.pop() {
            if node_id == NULL_NODE {
                continue;
            }
            let node = self.n(node_id);
            if !test_overlap_aabb(&node.aabb, &segment_aabb) {
                continue;
            }
            let c = node.aabb.center();
            let h = node.aabb.extents();
            let separation = abs(dot(v, p1 - c)) - dot(abs_v, h);
            if separation > 0.0 {
                continue;
            }
            if node.is_leaf() {
                let sub_input = RayCastInput {
                    p1: input.p1,
                    p2: input.p2,
                    max_fraction,
                };
                let value = callback(&sub_input, node_id);
                if value == 0.0 {
                    return;
                }
                if value > 0.0 {
                    max_fraction = value;
                    t = p1 + max_fraction * (p2 - p1);
                    segment_aabb = Aabb {
                        lower_bound: min2(p1, t),
                        upper_bound: max2(p1, t),
                    };
                }
            } else {
                stack.push(node.child1);
                stack.push(node.child2);
            }
        }
    }

    pub fn shift_origin(&mut self, new_origin: Vec2) {
        for n in &mut self.nodes {
            n.aabb.lower_bound -= new_origin;
            n.aabb.upper_bound -= new_origin;
        }
    }
}

/// `b2BroadPhase`: reports new overlapping pairs of moved proxies.
#[derive(Clone, Debug, Default)]
pub struct BroadPhase {
    tree: DynamicTree,
    proxy_count: i32,
    move_buffer: Vec<i32>,
    query_proxy_id: i32,
}

pub const NULL_PROXY: i32 = -1;

impl BroadPhase {
    pub fn new() -> Self {
        BroadPhase {
            tree: DynamicTree::new(),
            proxy_count: 0,
            move_buffer: Vec::new(),
            query_proxy_id: NULL_PROXY,
        }
    }

    pub fn create_proxy(&mut self, aabb: &Aabb, user_data: usize) -> i32 {
        let proxy_id = self.tree.create_proxy(aabb, user_data);
        self.proxy_count += 1;
        self.move_buffer.push(proxy_id);
        proxy_id
    }

    pub fn destroy_proxy(&mut self, proxy_id: i32) {
        for m in &mut self.move_buffer {
            if *m == proxy_id {
                *m = NULL_PROXY;
            }
        }
        self.proxy_count -= 1;
        self.tree.destroy_proxy(proxy_id);
    }

    pub fn move_proxy(&mut self, proxy_id: i32, aabb: &Aabb, displacement: Vec2) {
        if self.tree.move_proxy(proxy_id, aabb, displacement) {
            self.move_buffer.push(proxy_id);
        }
    }

    pub fn touch_proxy(&mut self, proxy_id: i32) {
        self.move_buffer.push(proxy_id);
    }

    pub fn fat_aabb(&self, proxy_id: i32) -> &Aabb {
        self.tree.fat_aabb(proxy_id)
    }

    pub fn user_data(&self, proxy_id: i32) -> usize {
        self.tree.user_data(proxy_id)
    }

    pub fn test_overlap(&self, a: i32, b: i32) -> bool {
        test_overlap_aabb(self.tree.fat_aabb(a), self.tree.fat_aabb(b))
    }

    pub fn proxy_count(&self) -> i32 {
        self.proxy_count
    }

    /// Collects the pairs of moved proxies (sorted, duplicates removed) and calls
    /// `add_pair(user_data_a, user_data_b)` for each.
    pub fn update_pairs(&mut self, mut add_pair: impl FnMut(usize, usize)) {
        let mut pairs: Vec<(i32, i32)> = Vec::new();
        let moved = std::mem::take(&mut self.move_buffer);
        for &id in &moved {
            self.query_proxy_id = id;
            if id == NULL_PROXY {
                continue;
            }
            let fat = *self.tree.fat_aabb(id);
            self.tree.query(&fat, |proxy_id| {
                if proxy_id != id {
                    pairs.push((proxy_id.min(id), proxy_id.max(id)));
                }
                true
            });
        }
        pairs.sort_unstable();
        pairs.dedup();
        for (a, b) in pairs {
            add_pair(self.tree.user_data(a), self.tree.user_data(b));
        }
    }

    pub fn query(&self, aabb: &Aabb, callback: impl FnMut(i32) -> bool) {
        self.tree.query(aabb, callback);
    }

    pub fn ray_cast(&self, input: &RayCastInput, callback: impl FnMut(&RayCastInput, i32) -> Real) {
        self.tree.ray_cast(input, callback);
    }

    pub fn tree_height(&self) -> i32 {
        self.tree.height()
    }

    pub fn shift_origin(&mut self, new_origin: Vec2) {
        self.tree.shift_origin(new_origin);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x: Real, y: Real, h: Real) -> Aabb {
        Aabb {
            lower_bound: vec2(x - h, y - h),
            upper_bound: vec2(x + h, y + h),
        }
    }

    #[test]
    fn tree_stays_valid_through_inserts_moves_and_removals() {
        let mut tree = DynamicTree::new();
        let ids: Vec<i32> = (0..100)
            .map(|i| {
                tree.create_proxy(
                    &square((i % 10) as Real * 3.0, (i / 10) as Real * 3.0, 0.5),
                    i,
                )
            })
            .collect();
        assert!(tree.validate());
        for (i, &id) in ids.iter().enumerate() {
            if i % 3 == 0 {
                tree.move_proxy(id, &square(100.0 + i as Real, 0.0, 0.5), vec2(1.0, 0.0));
            }
        }
        assert!(tree.validate());
        for &id in ids.iter().step_by(2) {
            tree.destroy_proxy(id);
        }
        assert!(tree.validate());
        let mut hits = 0;
        tree.query(&square(3.0, 0.0, 1.0), |_| {
            hits += 1;
            true
        });
        assert!(hits >= 1);
    }

    #[test]
    fn broadphase_reports_each_overlapping_pair_once_in_sorted_order() {
        let mut bp = BroadPhase::new();
        bp.create_proxy(&square(0.0, 0.0, 1.0), 10);
        bp.create_proxy(&square(1.0, 0.0, 1.0), 20);
        bp.create_proxy(&square(50.0, 0.0, 1.0), 30);
        let mut pairs = Vec::new();
        bp.update_pairs(|a, b| pairs.push((a, b)));
        assert_eq!(pairs, vec![(10, 20)]);
        let mut again = Vec::new();
        bp.update_pairs(|a, b| again.push((a, b)));
        assert!(again.is_empty(), "the move buffer was consumed");
    }

    #[test]
    fn ray_cast_visits_leaves_along_the_ray() {
        let mut tree = DynamicTree::new();
        tree.create_proxy(&square(5.0, 0.0, 0.5), 1);
        tree.create_proxy(&square(5.0, 10.0, 0.5), 2);
        let mut seen = Vec::new();
        tree.ray_cast(
            &RayCastInput {
                p1: vec2(0.0, 0.0),
                p2: vec2(10.0, 0.0),
                max_fraction: 1.0,
            },
            |_, id| {
                seen.push(id);
                -1.0
            },
        );
        assert_eq!(seen.len(), 1);
    }
}
