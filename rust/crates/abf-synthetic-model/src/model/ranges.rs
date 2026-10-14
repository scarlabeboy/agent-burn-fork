use super::*;

impl<'id> Model<'id> {
    pub fn begin(
        &mut self,
        grant: Grant<'id>,
        sampled: Object<'id>,
        range: Range,
        worker_deadline: u64,
    ) -> Result<Operation<'id>, Reason> {
        self.grant_live(grant.index)?; // A is this call, never a reusable poll
        if self.grants[grant.index].object != sampled.index {
            self.retire(grant)?;
            return Err(Reason::BindingMismatch);
        }
        if range.length == 0
            || range.length > 65536
            || range.domain_end > self.objects[sampled.index].sampled_size
            || range.domain_end > self.scopes[self.grants[grant.index].scope].range_end
            || range
                .start
                .checked_add(range.length)
                .is_none_or(|end| end > range.domain_end)
        {
            return Err(Reason::RangeInvalid);
        }
        if worker_deadline <= self.now {
            return Err(Reason::Fenced);
        }
        self.charge(&[(0, 2), (1, 1), (2, 1), (7, 4), (11, 1)])?; // read, both raw postchecks, cleanup
        let id = Operation::new(self.ops.len());
        self.ops.push(OpRow {
            grant: grant.index,
            object: sampled.index,
            range,
            deadline: worker_deadline,
            state: OpState::Admitted,
            fenced: false,
            copies: 0,
            pair: true,
            stage: true,
            payload: 0,
            capture: self.now,
        });
        self.counters.reads += 1;
        Ok(id)
    }
    pub fn duplicate_worker_copy(&mut self, op: Operation<'id>) -> Result<(), Reason> {
        self.grant_live(self.ops[op.index].grant)?;
        if self.ops[op.index].fenced || !self.ops[op.index].pair {
            return Err(Reason::Fenced);
        }
        self.charge(&[(0, 1), (7, 1)])?;
        self.ops[op.index].copies += 1;
        Ok(())
    }
    pub(super) fn close_pair(&mut self, i: usize) {
        if self.ops[i].pair {
            self.counters.post_samples += 2;
            self.resources.free(0, 2 + self.ops[i].copies);
            self.resources.free(1, 1);
            self.ops[i].pair = false;
            self.ops[i].copies = 0;
        }
    }
    pub(super) fn dispose(&mut self, i: usize) {
        assert!(
            !self.ops[i].pair,
            "outstanding read requires observed return"
        );
        if self.ops[i].stage {
            self.resources.free(2, 1);
            self.ops[i].stage = false;
        }
        if self.ops[i].state != OpState::Accepted {
            self.ops[i].state = OpState::Disposed;
        }
    }
    pub fn return_read(
        &mut self,
        op: Operation<'id>,
        sampled: Object<'id>,
        synthetic_payload: u32,
    ) -> Result<(), Reason> {
        if self.ops[op.index].state != OpState::Admitted {
            return Err(Reason::Replay);
        }
        self.close_pair(op.index); // mandatory raw post pair even after R/fence
        let g = self.ops[op.index].grant;
        if self.ops[op.index].object != sampled.index {
            self.retire(Grant::new(g))?;
            self.dispose(op.index);
            self.cleanup();
            return Err(Reason::BindingMismatch);
        }
        if self.ops[op.index].fenced || self.grant_live(g).is_err() {
            self.dispose(op.index);
        } else {
            self.ops[op.index].payload = synthetic_payload;
            self.ops[op.index].state = OpState::Staged;
        }
        self.cleanup();
        Ok(())
    }
    pub fn release(&mut self, op: Operation<'id>, receiver: u32) -> Result<Released<'id>, Reason> {
        let row = &self.ops[op.index];
        self.grant_live(row.grant)?;
        if row.fenced {
            return Err(Reason::Fenced);
        }
        let spec = self.scopes[self.grants[row.grant].scope].spec;
        if !spec.release || receiver != spec.receiver {
            return Err(Reason::Unauthorized);
        }
        if row.state != OpState::Staged {
            return Err(Reason::Replay);
        }
        self.charge(&[(7, 1)])?; // check + actual receiver reachability in one transition L
        self.ops[op.index].state = OpState::Released;
        self.resources.free(2, 1);
        self.ops[op.index].stage = false;
        self.counters.releases += 1;
        Ok(Released {
            index: op.index,
            brand: PhantomData,
            payload: self.ops[op.index].payload,
        })
    }
    pub fn accept(
        &mut self,
        released: Released<'id>,
        receiver: u32,
    ) -> Result<Observation<'id>, Reason> {
        let row = &self.ops[released.index];
        self.grant_live(row.grant)?;
        let spec = self.scopes[self.grants[row.grant].scope].spec;
        if row.fenced {
            return Err(Reason::Fenced);
        }
        if !spec.use_results || receiver != spec.receiver {
            return Err(Reason::Unauthorized);
        }
        if row.state != OpState::Released {
            return Err(Reason::Replay);
        }
        self.charge(&[(11, 1), (6, 1), (7, 1)])?; // P is actual bounded receiver acceptance
        let id = Observation::new(self.observations.len());
        let producer_unverified = !self.objects[self.ops[released.index].object].producer;
        self.observations.push(ObsRow {
            op: released.index,
            qualification: Qualification {
                inventory_partial: true,
                byte_partial: true,
                version_unproven: true,
                producer_unverified,
                identity_unresolved: true,
                ..Qualification::default()
            },
            revision: 1,
        });
        self.ops[released.index].state = OpState::Accepted;
        self.counters.uses += 1;
        Ok(id)
    }
    pub fn observation(&self, ob: Observation<'id>) -> Result<Qualification, Reason> {
        let row = &self.observations[ob.index];
        let mut q = row.qualification;
        if self.grant_live(self.ops[row.op].grant).is_err() {
            q.revoked = true;
            q.stale = true;
        }
        Ok(q)
    }
}
