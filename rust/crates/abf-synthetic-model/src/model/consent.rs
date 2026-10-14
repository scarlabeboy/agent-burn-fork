use super::*;

impl<'id> Model<'id> {
    /// Fixture boundary supplies only abstract objects, never external pins/handles.
    pub fn fixture_object(
        &mut self,
        lineage: Lineage,
        hint: u32,
        eligible: bool,
        producer: bool,
    ) -> Result<Object<'id>, Reason> {
        // Compatibility fixture: explicitly synthetic 64-unit sampled extent.
        self.fixture_object_with_size(lineage, hint, eligible, producer, 64)
    }
    /// Trusted synthetic sample fact, immutable for this object capability.
    pub fn fixture_object_with_size(
        &mut self,
        lineage: Lineage,
        hint: u32,
        eligible: bool,
        producer: bool,
        sampled_size: u32,
    ) -> Result<Object<'id>, Reason> {
        if self.fenced {
            return Err(Reason::Fenced);
        }
        self.charge(&[(11, 1)])?;
        let id = Object::new(self.objects.len());
        self.objects.push(Obj {
            lineage,
            hint,
            eligible,
            producer,
            holder: self.holder,
            sampled_size,
        });
        Ok(id)
    }
    /// Explicit synthetic approval-boundary injection; never authenticates a real user.
    pub fn approve(&mut self, spec: ScopeSpec) -> Result<Scope<'id>, Reason> {
        // Compatibility fixture: independently approved synthetic ceiling of 64.
        self.approve_with_range(spec, 64)
    }
    /// Explicit synthetic approval of a finite range ceiling, not a native permission.
    pub fn approve_with_range(
        &mut self,
        spec: ScopeSpec,
        range_end: u32,
    ) -> Result<Scope<'id>, Reason> {
        if self.fenced {
            return Err(Reason::Fenced);
        }
        if !spec.metadata || spec.valid_until <= self.now {
            return Err(Reason::Unauthorized);
        }
        self.charge(&[(0, 2), (7, 1), (11, 1)])?;
        let id = Scope::new(self.scopes.len());
        self.scopes.push(ScopeRow {
            spec,
            range_end,
            epoch: self.broker,
            active: true,
            owners: true,
        });
        Ok(id)
    }
    pub fn renew(&mut self, scope: Scope<'id>, spec: ScopeSpec) -> Result<Scope<'id>, Reason> {
        if spec.lineage != self.scopes[scope.index].spec.lineage {
            return Err(Reason::Unauthorized);
        }
        // Reject invalid terms before retirement. Budget failure after R is
        // intentionally fail-closed: no rollback or credit refill is promised.
        if self.fenced {
            return Err(Reason::Fenced);
        }
        if !spec.metadata || spec.valid_until <= self.now {
            return Err(Reason::Unauthorized);
        }
        let range_end = self.scopes[scope.index].range_end;
        self.retire_scope(scope)?;
        self.approve_with_range(spec, range_end) // no reset of credit or declines
    }
    pub fn discover(
        &mut self,
        scope: Scope<'id>,
        object: Object<'id>,
        present: bool,
        replacement: bool,
    ) -> Result<Proposal<'id>, Reason> {
        self.scope_live(scope.index)?;
        self.charge(&[(7, 1)])?; // rejected/ineligible candidates cost work too
        self.counters.discoveries += 1;
        let obj = &self.objects[object.index];
        if obj.holder != self.holder
            || obj.lineage != self.scopes[scope.index].spec.lineage
            || !obj.eligible
        {
            return Err(Reason::Ineligible);
        }
        self.charge(&[(0, 1), (3, 1), (11, 1), (7, 1)])?; // proposal owner plus pre-reserved disposal
        let id = Proposal::new(self.proposals.len());
        self.proposals.push(ProposalRow {
            scope: scope.index,
            object: object.index,
            present,
            replacement,
            held: true,
            prompted: false,
        });
        Ok(id)
    }
    pub fn discard(&mut self, p: Proposal<'id>) -> Result<(), Reason> {
        let row = &mut self.proposals[p.index];
        if !row.held {
            return Err(Reason::Replay);
        }
        row.held = false;
        self.resources.free(0, 1);
        self.resources.free(3, 1);
        Ok(())
    }
    pub fn fixture_retained_hint(&mut self, scope: Scope<'id>, hint: u32) -> Result<(), Reason> {
        self.scope_live(scope.index)?;
        let key = (self.scopes[scope.index].spec.lineage, hint);
        if !self.hints.contains(&key) {
            self.charge(&[(4, 1)])?;
            self.hints.push(key);
        }
        Ok(())
    }
    pub fn evict_hint(&mut self, scope: Scope<'id>, hint: u32) {
        let key = (self.scopes[scope.index].spec.lineage, hint);
        if let Some(i) = self.hints.iter().position(|x| *x == key) {
            self.hints.remove(i);
            self.resources.free(4, 1);
        }
    }
    pub fn reencounter(
        &self,
        scope: Scope<'id>,
        object: Object<'id>,
    ) -> Result<Grant<'id>, Reason> {
        self.scope_live(scope.index)?;
        self.grants
            .iter()
            .enumerate()
            .find(|(_, g)| g.active && g.scope == scope.index && g.object == object.index)
            .map(|(i, _)| Grant::new(i))
            .ok_or(Reason::Retired)
    }
    pub fn admit(&mut self, p: Proposal<'id>, manual: bool) -> Result<Grant<'id>, Reason> {
        let row = &self.proposals[p.index];
        self.scope_live(row.scope)?; // atomic child insertion shares this serialized call
        if !row.held {
            return Err(Reason::Replay);
        }
        let spec = self.scopes[row.scope].spec;
        let obj = &self.objects[row.object];
        self.suppression(spec.lineage, obj.hint)?;
        let retained = self.hints.contains(&(spec.lineage, obj.hint));
        if let Err(codes) = spec.policy.dispositions(row.present, retained) {
            return Err(codes[0]);
        }
        if !obj.producer && !spec.policy.allow_unverified {
            return Err(Reason::ProducerUnverified);
        }
        if spec.policy.require_newness {
            return Err(Reason::TemporalNewnessUnproven);
        }
        if row.replacement && !spec.policy.allow_replacement {
            return Err(Reason::ReplacementDenied);
        }
        if !spec.bytes
            || (manual && (!spec.policy.manual || !row.prompted))
            || (!manual && !spec.policy.automatic)
        {
            return Err(Reason::Unauthorized);
        }
        if self
            .grants
            .iter()
            .any(|g| g.active && g.object == row.object)
        {
            return Err(Reason::DuplicateLiveObjectGrant);
        }
        let (scope, object) = (row.scope, row.object);
        self.charge(&[(11, 1), (7, 1)])?;
        self.proposals[p.index].held = false;
        self.resources.free(3, 1); // canonical F transfers, does not close
        let id = Grant::new(self.grants.len());
        self.grants.push(GrantRow {
            scope,
            object,
            active: true,
            owner: true,
        });
        self.counters.admissions += 1;
        // Hint overflow never widens permission: a miss applies R+U.
        let _ = self.fixture_retained_hint(Scope::new(scope), self.objects[object].hint);
        Ok(id)
    }
    pub fn request_prompt(
        &mut self,
        p: Proposal<'id>,
        receiver: u32,
        user_requested: bool,
    ) -> Result<Prompt, Reason> {
        let row = &self.proposals[p.index];
        self.scope_live(row.scope)?;
        let spec = self.scopes[row.scope].spec;
        if !user_requested
            || !spec.display
            || receiver != spec.display_receiver
            || !spec.policy.manual
        {
            return Err(Reason::Unauthorized);
        }
        if !row.held || row.prompted {
            return Err(Reason::Replay);
        }
        spec.policy.validate_configuration()?;
        self.suppression(spec.lineage, self.objects[row.object].hint)?;
        self.charge(&[(8, 1), (9, 1), (7, 1)])?;
        self.proposals[p.index].prompted = true;
        self.counters.prompts += 1;
        let obj = &self.objects[self.proposals[p.index].object];
        Ok(Prompt {
            name_hint: obj.hint,
            producer_unverified: !obj.producer,
        })
    }
    pub fn decline(&mut self, p: Proposal<'id>) -> Result<(), Reason> {
        let row = &self.proposals[p.index];
        self.scope_live(row.scope)?;
        if !row.held {
            return Err(Reason::Replay);
        }
        let lineage = self.scopes[row.scope].spec.lineage;
        let key = (lineage, self.objects[row.object].hint);
        if !self.declines.contains(&key) {
            if self.charge(&[(5, 1)]).is_err() {
                // One latch per existing scope lineage, bounded by scope registry.
                if !self.overflow.contains(&lineage) {
                    self.overflow.push(lineage);
                }
                self.discard(p)?;
                return Err(Reason::ManualSuppressionOverflow);
            }
            self.declines.push(key);
        }
        self.discard(p)
    }
    pub fn clear_decline(
        &mut self,
        lineage: Lineage,
        hint: u32,
        separate_user_approval: bool,
    ) -> Result<(), Reason> {
        if !separate_user_approval {
            return Err(Reason::Unauthorized);
        }
        if !self.consent_continuity {
            return Err(Reason::ContinuityLost);
        }
        self.charge(&[(10, 1), (7, 1)])?;
        if let Some(i) = self.declines.iter().position(|x| *x == (lineage, hint)) {
            self.declines.remove(i);
            self.resources.free(5, 1);
        }
        // Does not clear overflow, grant authority, or replenish prompt credit.
        Ok(())
    }
    pub fn lose_consent_continuity(&mut self) {
        self.consent_continuity = false;
    }
}
