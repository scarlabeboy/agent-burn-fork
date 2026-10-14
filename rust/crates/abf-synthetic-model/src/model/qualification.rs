use super::*;

impl<'id> Model<'id> {
    pub fn derive(&mut self, observations: &[Observation<'id>]) -> Result<Derived<'id>, Reason> {
        if observations.is_empty() {
            return Err(Reason::LineageUnknown);
        }
        for ob in observations {
            let op = &self.ops[self.observations[ob.index].op];
            self.grant_live(op.grant)?;
            if !self.scopes[self.grants[op.grant].scope].spec.use_results {
                return Err(Reason::Unauthorized);
            }
        }
        let mut unique = Vec::new();
        for ob in observations {
            if !unique.contains(&ob.index) {
                unique.push(ob.index);
            }
        }
        unique.sort_unstable();
        let count = u32::try_from(unique.len()).map_err(|_| Reason::Exhausted)?;
        self.charge(&[(6, count), (11, 1), (7, count)])?;
        let id = Derived::new(self.derived.len());
        self.derived.push(DerivedRow {
            observations: unique,
            lineage_known: true,
        });
        Ok(id)
    }
    /// Computer contains only underlying observation lineage, never collection rights.
    pub fn computer(&mut self, inputs: &[Derived<'id>]) -> Result<Derived<'id>, Reason> {
        if inputs.is_empty() || inputs.iter().any(|d| !self.derived[d.index].lineage_known) {
            return Err(Reason::LineageUnknown);
        }
        let mut observations = Vec::new();
        for input in inputs {
            for &i in &self.derived[input.index].observations {
                let op = &self.ops[self.observations[i].op];
                self.grant_live(op.grant)?;
                if !self.scopes[self.grants[op.grant].scope].spec.use_results {
                    return Err(Reason::Unauthorized);
                }
                if !observations.contains(&i) {
                    observations.push(i);
                }
            }
        }
        observations.sort_unstable();
        let count = u32::try_from(observations.len()).map_err(|_| Reason::Exhausted)?;
        self.charge(&[(6, count), (11, 1), (7, count)])?;
        let id = Derived::new(self.derived.len());
        self.derived.push(DerivedRow {
            observations,
            lineage_known: true,
        });
        Ok(id)
    }
    /// Retained evidence inspection, not a new receiver use. No qualification cache.
    pub fn view(&self, d: Derived<'id>) -> Result<View, Reason> {
        let row = &self.derived[d.index];
        if !row.lineage_known {
            return Err(Reason::LineageUnknown);
        }
        let mut result = View {
            qualification: Qualification::default(),
            observations: row.observations.len(),
            sampled_bytes: 0,
            environments: Vec::new(),
            ranges: Vec::new(),
        };
        let mut spans = Vec::new();
        for &i in &row.observations {
            let op = &self.ops[self.observations[i].op];
            let env = self.objects[op.object].lineage.environment;
            if !result.environments.contains(&env) {
                result.environments.push(env);
            }
            result
                .qualification
                .inherit(self.observation(Observation::new(i))?);
            result.ranges.push((env, op.range, op.capture));
            spans.push((op.object, op.range.start, op.range.start + op.range.length));
        }
        spans.sort_unstable();
        let mut last: Option<(usize, u32)> = None;
        for (object, start, end) in spans {
            let prior = last
                .filter(|&(o, _)| o == object)
                .map_or(start, |(_, e)| e.max(start));
            result.sampled_bytes = result
                .sampled_bytes
                .checked_add(end.saturating_sub(prior))
                .ok_or(Reason::Exhausted)?;
            last = Some((object, end.max(prior)));
        }
        result.environments.sort_unstable();
        Ok(result)
    }
    pub fn use_result(&mut self, d: Derived<'id>, receiver: u32) -> Result<View, Reason> {
        if !self.derived[d.index].lineage_known {
            return Err(Reason::LineageUnknown);
        }
        for &i in &self.derived[d.index].observations {
            let op = &self.ops[self.observations[i].op];
            self.grant_live(op.grant)?;
            let spec = self.scopes[self.grants[op.grant].scope].spec;
            if !spec.use_results || spec.receiver != receiver {
                return Err(Reason::Unauthorized);
            }
        }
        self.charge(&[(7, 1)])?;
        let view = self.view(d)?; // P checks rights + current qualification in one step
        self.counters.uses += 1;
        Ok(view)
    }
    pub fn downgrade(
        &mut self,
        observation: Observation<'id>,
        stale: bool,
        disputed: bool,
    ) -> Result<(), Reason> {
        // Restrictive facts modify existing storage in constant space. They must
        // never be lost because ordinary work or retained lineage is exhausted.
        let row = &mut self.observations[observation.index];
        row.qualification.stale |= stale;
        row.qualification.disputed |= disputed;
        if let Some(revision) = row.revision.checked_add(1) {
            row.revision = revision;
        } else {
            for derived in &mut self.derived {
                derived.lineage_known = false;
            }
            return Err(Reason::LineageUnknown);
        }
        Ok(())
    }
    pub fn lose_lineage(&mut self, derived: Derived<'id>) -> Result<(), Reason> {
        let _ = derived;
        for row in &mut self.derived {
            row.lineage_known = false;
        }
        Ok(())
    }
}
