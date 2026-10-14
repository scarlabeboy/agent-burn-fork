use crate::Reason;

/// Releasable occupancy: owners, workers, stages, proposals, hints, declines.
/// Class 6 is retained lineage storage; this model has no retention-authorized release.
/// Cumulative: work, prompts, batches, clear actions, registry/replay records.
/// Array order is stable only within this synthetic candidate.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub caps: [u32; 12],
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Resources {
    pub held: [u32; 12],
    pub spent: [u32; 12],
    pub peak: [u32; 12],
}
impl Resources {
    pub(crate) fn reserve(&mut self, limits: Limits, costs: &[(usize, u32)]) -> Result<(), Reason> {
        let mut next = self.held;
        for &(kind, count) in costs {
            next[kind] = next[kind].checked_add(count).ok_or(Reason::Exhausted)?;
            if next[kind] > limits.caps[kind] {
                return Err(Reason::Exhausted);
            }
        }
        let mut spent = self.spent;
        for &(kind, count) in costs {
            spent[kind] = spent[kind].checked_add(count).ok_or(Reason::Exhausted)?;
        }
        self.spent = spent;
        self.held = next;
        for (i, v) in next.iter().enumerate() {
            self.peak[i] = self.peak[i].max(*v);
        }
        Ok(())
    }
    pub(crate) fn free(&mut self, kind: usize, count: u32) {
        assert!(kind < 7, "cumulative credit cannot be returned");
        self.held[kind] = self.held[kind].checked_sub(count).expect("accounted owner");
    }
}
