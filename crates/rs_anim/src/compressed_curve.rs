//! Linked key candidates for adaptive cubic fitting.
use super::*;
pub(super) struct Curve {
    pub(super) kind: ChannelKind,
    pub(super) keys: Vec<Key>,
    pub(super) values: Vec<Vec4>,
    pub(super) expected: Vec<Vec4>,
    pub(super) prev: Vec<usize>,
    pub(super) next: Vec<usize>,
    pub(super) active: Vec<bool>,
    pub(super) versions: Vec<u32>,
    pub(super) alive: usize,
}
pub(super) fn ordered_cost(value: f32) -> u32 {
    let bits = value.to_bits();
    if bits >> 31 != 0 {
        !bits
    } else {
        bits ^ 0x8000_0000
    }
}
impl Curve {
    pub(super) fn pair_cost(&mut self, node: usize, audit: &[u16]) -> f32 {
        let last = self.next[node];
        let a = self.prev[node];
        let b = self.next[last];
        let start = self.prev[a];
        let end = self.next[b];
        let before = self.squared_residual(start, end, audit);
        self.next[a] = b;
        self.prev[b] = a;
        let after = self.squared_residual(start, end, audit);
        self.next[a] = node;
        self.prev[b] = last;
        ((after - before) / 2.) as f32
    }
    pub(super) fn can_remove_pair(&self, node: usize) -> bool {
        node > 0
            && self.next[node] < self.keys.len() - 1
            && self.keys[node].time == self.keys[self.next[node]].time
    }
    pub(super) fn squared_residual(&self, start: usize, end: usize, audit: &[u16]) -> f64 {
        let lo = audit.partition_point(|&t| t < self.keys[start].time);
        let hi = audit.partition_point(|&t| t <= self.keys[end].time);
        let mut key = start;
        let mut sum = 0.;
        for (sample, &time) in audit.iter().enumerate().take(hi).skip(lo) {
            while self.next[key] != key && self.keys[self.next[key]].time <= time {
                key = self.next[key];
            }
            let ids = [
                self.prev[key],
                key,
                self.next[key],
                self.next[self.next[key]],
            ];
            let actual = self.kind.sample(
                time,
                ids.map(|i| self.keys[i].time),
                ids.map(|i| self.values[i]),
            );
            sum += (self.kind.error(self.expected[sample], actual) as f64).powi(2);
        }
        sum
    }
    pub(super) fn cost(&mut self, node: usize, audit: &[u16]) -> f32 {
        let a = self.prev[node];
        let b = self.next[node];
        let start = self.prev[a];
        let end = self.next[b];
        let before = self.squared_residual(start, end, audit);
        self.next[a] = b;
        self.prev[b] = a;
        let after = self.squared_residual(start, end, audit);
        self.next[a] = node;
        self.prev[b] = node;
        (after - before) as f32
    }
    pub(super) fn remove(&mut self, node: usize) -> Vec<usize> {
        let mut left = self.prev[node];
        let mut right = self.next[node];
        self.next[left] = right;
        self.prev[right] = left;
        self.active[node] = false;
        self.alive -= 1;
        let mut changed = Vec::with_capacity(6);
        for _ in 0..4 {
            changed.push(left);
            changed.push(right);
            left = self.prev[left];
            right = self.next[right];
        }
        changed.sort_unstable();
        changed.dedup();
        changed.retain(|&i| i != 0 && i != self.keys.len() - 1);
        for &i in &changed {
            self.versions[i] += 1;
        }
        changed
    }
}
