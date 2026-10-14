use super::*;

impl<'id> Model<'id> {
    pub fn retire(&mut self, grant: Grant<'id>) -> Result<(), Reason> {
        self.grants[grant.index].active = false; // R precedes disposal/free
        for i in 0..self.ops.len() {
            if self.ops[i].grant == grant.index && !self.ops[i].pair {
                self.dispose(i);
            }
        }
        self.cleanup();
        Ok(())
    }
    pub fn pressure_retire(&mut self, grant: Grant<'id>) -> Result<(), Reason> {
        if !self.scopes[self.grants[grant.index].scope]
            .spec
            .policy
            .retirement
        {
            return Err(Reason::Unauthorized);
        }
        self.charge(&[(7, 1)])?;
        self.retire(grant)
    }
    pub fn retire_scope(&mut self, scope: Scope<'id>) -> Result<(), Reason> {
        self.scopes[scope.index].active = false;
        for i in 0..self.grants.len() {
            if self.grants[i].scope == scope.index {
                self.retire(Grant::new(i))?;
            }
        }
        for i in 0..self.proposals.len() {
            if self.proposals[i].scope == scope.index && self.proposals[i].held {
                self.discard(Proposal::new(i))?;
            }
        }
        self.cleanup();
        Ok(())
    }
    pub fn scope_mismatch(&mut self, scope: Scope<'id>) -> Result<(), Reason> {
        self.retire_scope(scope)
    }
    pub fn transient(&mut self, grant: Grant<'id>) -> Result<(), Reason> {
        self.grant_live(grant.index)
    }
    pub(super) fn cleanup(&mut self) {
        for (i, g) in self.grants.iter_mut().enumerate() {
            if !g.active && g.owner && !self.ops.iter().any(|o| o.grant == i && o.pair) {
                g.owner = false;
                self.resources.free(0, 1);
            }
        }
        for (i, s) in self.scopes.iter_mut().enumerate() {
            if !s.active && s.owners && !self.grants.iter().any(|g| g.scope == i && g.owner) {
                s.owners = false;
                self.resources.free(0, 2);
            }
        }
    }
    pub fn fence_worker(&mut self, op: Operation<'id>) -> Result<(), Reason> {
        self.ops[op.index].fenced = true;
        if !self.ops[op.index].pair {
            self.dispose(op.index);
        }
        Ok(())
    }
    pub fn session_loss(&mut self) {
        self.fenced = true;
        for i in 0..self.ops.len() {
            self.ops[i].fenced = true;
        }
        for i in 0..self.scopes.len() {
            let _ = self.retire_scope(Scope::new(i));
        }
        self.cleanup();
    }
    /// Explicit clock step: a deadline fences but never proves a read returned.
    /// Outstanding owners/copies/stages remain charged until observed cleanup.
    pub fn step(&mut self, now: u64) -> Result<(), Reason> {
        if now < self.now {
            self.session_loss();
            return Err(Reason::Fenced);
        }
        self.now = now;
        for i in 0..self.scopes.len() {
            if self.scopes[i].active && now >= self.scopes[i].spec.valid_until {
                self.retire_scope(Scope::new(i))?;
            }
        }
        for i in 0..self.ops.len() {
            if now >= self.ops[i].deadline {
                self.ops[i].fenced = true;
                if !self.ops[i].pair {
                    self.dispose(i);
                }
            }
        }
        self.cleanup();
        Ok(())
    }
    pub fn restart(
        &mut self,
        evidence: IndependentEvidence,
        explicit_reenable: bool,
    ) -> Result<(), Reason> {
        if !explicit_reenable {
            return Err(Reason::Unauthorized);
        }
        if !self.fenced
            || self.resources.held[0] != 0
            || self.resources.held[1] != 0
            || self.resources.held[2] != 0
            || matches!(
                evidence,
                IndependentEvidence::SelfReport | IndependentEvidence::PartialInventory
            )
        {
            return Err(Reason::RecoveryUnavailable);
        }
        self.broker = self.broker.checked_add(1).ok_or(Reason::Exhausted)?;
        self.holder = self.holder.checked_add(1).ok_or(Reason::Exhausted)?;
        self.quiescent = matches!(evidence, IndependentEvidence::CompleteDrain);
        self.fenced = false;
        Ok(())
    }
    pub fn quiescence_confirmed(&self) -> bool {
        self.quiescent
    }
}
