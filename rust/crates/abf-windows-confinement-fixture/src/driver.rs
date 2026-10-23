use crate::model::*;
use std::{fmt, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    List,
    Open,
    Read,
}
#[derive(Clone, Copy, Debug)]
pub enum Completion {
    Success,
    Denied,
    Failed,
    Partial(u8),
    Transient,
}
#[derive(Clone, Copy, Debug)]
pub enum Step {
    Issue,
    Return(Completion),
    Release,
    Retry,
    Cancel,
    ObserveStop,
    Time(u64),
    Fault(Object, Fault),
    Substitute(Object, usize, Object),
    ReplaceRoot,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cancellation {
    Clear,
    Requested,
    Pending,
    Acknowledged,
}
#[derive(Clone, PartialEq, Eq)]
struct Provenance {
    binding: Binding,
    root: Object,
    volume: u64,
    root_revision: u64,
}
#[derive(Clone, PartialEq, Eq)]
pub struct Evidence {
    code: Code,
    provenance: Option<Provenance>,
    ledger: Ledger,
    cancel: Cancellation,
    bytes: [u8; 64],
    len: usize,
    admitted: u64,
    opened: u64,
    closed: u64,
    live: u64,
    sentinel: u64,
    operations: [Option<Operation>; 32],
    op_len: usize,
}
impl Evidence {
    pub fn binding(&self) -> Option<Binding> {
        self.provenance.as_ref().map(|p| p.binding)
    }
    pub fn bound_root(&self) -> Option<Object> {
        self.provenance.as_ref().map(|p| p.root)
    }
    pub fn code(&self) -> Code {
        self.code
    }
    pub fn ledger(&self) -> Ledger {
        self.ledger
    }
    pub fn cancellation(&self) -> Cancellation {
        self.cancel
    }
    /// Explicit bounded sensitive access; diagnostics never include this data.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
    pub fn admitted(&self) -> u64 {
        self.admitted
    }
    pub fn opened(&self) -> u64 {
        self.opened
    }
    pub fn closed(&self) -> u64 {
        self.closed
    }
    pub fn live(&self) -> u64 {
        self.live
    }
    pub fn sentinel_accesses(&self) -> u64 {
        self.sentinel
    }
    pub fn operations(&self) -> &[Option<Operation>] {
        &self.operations[..self.op_len]
    }
}
impl fmt::Debug for Evidence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Evidence([redacted])")
    }
}
#[derive(Clone, Copy)]
struct Candidate {
    id: Object,
    parent: Option<Object>,
    ordinal: usize,
    revision: u64,
    depth: u64,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Ready,
    Pending,
    Returned,
    Retry,
    Done,
}
pub struct Driver {
    world: FakeWorld,
    root: Option<FakeRoot>,
    limits: Limits,
    e: Evidence,
    phase: Phase,
    op: Operation,
    current: Candidate,
    queue: [Option<Candidate>; 16],
    head: usize,
    tail: usize,
    chunk: [u8; 8],
    chunk_len: usize,
    listing: Node,
    partial: bool,
    now: u64,
    deferred: Option<Code>,
    child_handle: bool,
}
impl fmt::Debug for Driver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Driver([redacted])")
    }
}
impl Driver {
    /// Takes ownership of a fake world. Clones share fictional allocation identity,
    /// not mutable state. Each driver is a separate deterministic replay.
    pub fn new(
        world: FakeWorld,
        approval: Option<&Selection>,
        binding: Binding,
        limits: Limits,
        selected: bool,
        fail_allocation: bool,
    ) -> Self {
        let id = world.root();
        let listing = world.node(id).expect("world always has its root slot");
        let mut d = Self {
            world,
            root: None,
            limits,
            e: Evidence {
                code: Code::Running,
                provenance: None,
                ledger: Ledger::default(),
                cancel: Cancellation::Clear,
                bytes: [0; 64],
                len: 0,
                admitted: 0,
                opened: 0,
                closed: 0,
                live: 0,
                sentinel: 0,
                operations: [None; 32],
                op_len: 0,
            },
            phase: Phase::Ready,
            op: Operation::List,
            current: Candidate {
                id,
                parent: None,
                ordinal: 0,
                revision: 0,
                depth: 0,
            },
            queue: [None; 16],
            head: 0,
            tail: 0,
            chunk: [0; 8],
            chunk_len: 0,
            listing,
            partial: false,
            now: 0,
            deferred: None,
            child_handle: false,
        };
        let result = (|| {
            if !selected {
                return Err(Code::NoWork);
            }
            let s = approval.ok_or(Code::Unauthorized)?;
            if !Arc::ptr_eq(&d.world.identity, &s.identity)
                || binding != s.binding
                || id != s.root
                || binding.environment != Environment::Windows
            {
                return Err(Code::BindingMismatch);
            }
            if fail_allocation {
                return Err(Code::Allocation);
            }
            // Graph count includes the root; the reserved sentinel is charged too.
            d.charge(Resource::Graph, d.world.len as u64)?;
            let edges = d.world.nodes.iter().flatten().map(|n| n.len as u64).sum();
            d.charge(Resource::Edges, edges)?;
            d.root = Some(FakeRoot {
                selection: Selection {
                    identity: s.identity.clone(),
                    binding: s.binding,
                    root: s.root,
                    volume: s.volume,
                    revision: s.revision,
                },
            });
            d.validate(d.current)?;
            d.e.provenance = Some(Provenance {
                binding: s.binding,
                root: s.root,
                volume: s.volume,
                root_revision: s.revision,
            });
            d.e.ledger.peak(d.limits, Resource::Handles, 1)?;
            d.e.opened = 1;
            d.e.live = 1;
            Ok(())
        })();
        if let Err(c) = result {
            d.stop(c);
        }
        d
    }
    /// A snapshot cannot mutate the driver or release staged data.
    pub fn evidence(&self) -> Evidence {
        self.e.clone()
    }
    fn charge(&mut self, r: Resource, n: u64) -> Result<(), Code> {
        self.e.ledger.charge(self.limits, r, n)
    }
    fn validate(&self, c: Candidate) -> Result<(), Code> {
        let s = &self.root.as_ref().ok_or(Code::Unauthorized)?.selection;
        if self.world.root() != s.root || self.world.node(s.root)?.revision != s.revision {
            return Err(Code::BindingMismatch);
        }
        self.world.inspect(c.id, s.root, s.volume)?;
        if let Some(p) = c.parent {
            let n = self.world.node(p)?;
            if n.revision != c.revision || n.children.get(c.ordinal) != Some(&Some(c.id)) {
                return Err(Code::BindingMismatch);
            }
        }
        Ok(())
    }
    fn close_child(&mut self) {
        if self.child_handle {
            self.e.closed += 1;
            self.e.live -= 1;
            self.child_handle = false;
        }
    }
    fn stop(&mut self, code: Code) {
        if self.e.cancel != Cancellation::Clear {
            self.e.cancel = Cancellation::Acknowledged;
        }
        self.close_child();
        self.e.closed += self.e.live;
        self.e.live = 0;
        self.chunk.fill(0);
        self.chunk_len = 0;
        self.e.code = code;
        self.phase = Phase::Done;
    }
    fn observe_cancel(&mut self) -> bool {
        if self.e.cancel == Cancellation::Clear {
            return false;
        }
        if self.phase == Phase::Pending {
            self.e.cancel = Cancellation::Pending;
            self.e.code = self.deferred.unwrap_or(Code::CancelPending);
        } else {
            self.e.cancel = Cancellation::Acknowledged;
            self.stop(Code::Cancelled);
        }
        true
    }
    pub fn step(&mut self, step: Step) {
        if self.phase == Phase::Done {
            return;
        }
        // Stop/return/cleanup are out of the work budget: reserve cannot be exhausted.
        match step {
            Step::Cancel => {
                if self.e.cancel == Cancellation::Clear {
                    self.e.cancel = Cancellation::Requested;
                }
                return;
            }
            Step::ObserveStop => {
                self.observe_cancel();
                return;
            }
            Step::Time(t) => {
                if t < self.now {
                    self.fail(Code::InvalidStep);
                    return;
                }
                self.now = t;
                if t >= 10 && self.e.cancel == Cancellation::Clear {
                    self.e.cancel = Cancellation::Requested;
                }
                return;
            }
            _ => {}
        }
        if self.e.cancel != Cancellation::Clear && !matches!(step, Step::Return(_)) {
            self.observe_cancel();
            return;
        }
        let result = self.advance(step);
        if let Err(c) = result {
            self.fail(c);
        }
    }
    // A protocol/budget failure with an outstanding operation must await its return.
    fn fail(&mut self, c: Code) {
        if self.phase == Phase::Pending {
            let first = *self.deferred.get_or_insert(c);
            self.e.code = first;
        } else {
            self.stop(c);
        }
    }
    fn advance(&mut self, step: Step) -> Result<(), Code> {
        // Return must always be consumable even after budget or protocol failure.
        if !matches!(step, Step::Return(_)) {
            self.charge(Resource::Steps, 1)?;
        }
        match step {
            Step::Fault(id, f) => self.world.fault(id, f),
            Step::Substitute(p, i, t) => self.world.substitute(p, i, t),
            Step::ReplaceRoot => {
                self.world.replace_root_with_sentinel();
                Ok(())
            }
            Step::Issue => self.issue(),
            Step::Return(c) => self.returned(c),
            Step::Release => self.release(),
            Step::Retry => {
                if self.phase != Phase::Retry {
                    return Err(Code::InvalidStep);
                }
                self.charge(Resource::Retries, 1)?;
                self.phase = Phase::Ready;
                Ok(())
            }
            Step::Cancel | Step::ObserveStop | Step::Time(_) => unreachable!(),
        }
    }
    fn issue(&mut self) -> Result<(), Code> {
        if self.phase != Phase::Ready {
            return Err(Code::InvalidStep);
        }
        self.validate(self.current)?;
        // Reserve a possible returned handle before issue; failed work is not refunded.
        let mut issue_ledger = self.e.ledger;
        if self.op == Operation::Open {
            issue_ledger.peak(
                self.limits,
                Resource::Handles,
                self.e.live.checked_add(1).ok_or(Code::Budget)?,
            )?;
        }
        if self.op == Operation::Read {
            let mut reserve = issue_ledger;
            reserve.charge(self.limits, Resource::Returned, 8)?;
            issue_ledger.charge(self.limits, Resource::Requested, 8)?;
        }
        issue_ledger.charge(self.limits, Resource::Operations, 1)?;
        self.e.ledger = issue_ledger;
        self.e.operations[self.e.op_len] = Some(self.op);
        self.e.op_len += 1;
        // The only simulated target access point. Reparse/alias/outside refusal
        // happens before this point; target lookup/traversal is never attempted.
        if self.world.node(self.current.id)?.outside {
            self.e.sentinel += 1;
        }
        self.phase = Phase::Pending;
        Ok(())
    }
    fn returned(&mut self, c: Completion) -> Result<(), Code> {
        if self.phase != Phase::Pending {
            return Err(Code::InvalidStep);
        }
        self.phase = Phase::Returned;
        // A successful open returns an owned handle even on a late cancelled result.
        if self.op == Operation::Open && matches!(c, Completion::Success) {
            self.e.opened += 1;
            self.e.live += 1;
            self.child_handle = true;
        }
        if self.op == Operation::Read && matches!(c, Completion::Success) {
            let n = self.world.node(self.current.id)?;
            self.charge(Resource::Returned, n.bytes as u64)?;
            self.chunk = n.data;
            self.chunk_len = n.bytes;
        }
        if let Some(code) = self.deferred.take() {
            self.stop(code);
            return Ok(());
        }
        if self.observe_cancel() {
            return Ok(());
        }
        self.charge(Resource::Steps, 1)?;
        self.validate(self.current)?;
        match c {
            Completion::Denied => Err(Code::Denied),
            Completion::Failed => Err(match self.op {
                Operation::List => Code::ListFailed,
                Operation::Open => Code::OpenFailed,
                Operation::Read => Code::ReadFailed,
            }),
            Completion::Transient => {
                self.phase = Phase::Retry;
                Ok(())
            }
            Completion::Partial(n) => {
                if self.op != Operation::List {
                    return Err(Code::InvalidStep);
                }
                self.listing = self.world.node(self.current.id)?;
                if n as usize > self.listing.len {
                    return Err(Code::InvalidStep);
                }
                self.listing.len = n as usize;
                self.partial = true;
                Ok(())
            }
            Completion::Success => {
                if self.op == Operation::List {
                    self.listing = self.world.node(self.current.id)?;
                }
                Ok(())
            }
        }
    }
    fn release(&mut self) -> Result<(), Code> {
        if self.phase != Phase::Returned {
            return Err(Code::InvalidStep);
        }
        self.validate(self.current)?;
        match self.op {
            Operation::List => {
                for ordinal in 0..self.listing.len {
                    self.charge(Resource::Examined, 1)?;
                    let id = self.listing.children[ordinal].ok_or(Code::BindingMismatch)?;
                    let c = Candidate {
                        id,
                        parent: Some(self.current.id),
                        ordinal,
                        revision: self.listing.revision,
                        depth: self.current.depth.checked_add(1).ok_or(Code::Budget)?,
                    };
                    // Reserve capacity for a possible rejection before inspection.
                    let mut rejection_reserve = self.e.ledger;
                    rejection_reserve.charge(self.limits, Resource::Rejected, 1)?;
                    if let Err(code) = self.validate(c) {
                        self.charge(Resource::Rejected, 1)?;
                        return Err(code);
                    }
                    self.e.ledger.peak(self.limits, Resource::Depth, c.depth)?;
                    let n = self.world.node(id)?;
                    if !n.directory {
                        self.charge(Resource::Sources, 1)?;
                        self.e.admitted += 1;
                    }
                    if self.tail == self.queue.len() {
                        return Err(Code::Budget);
                    }
                    self.queue[self.tail] = Some(c);
                    self.tail += 1;
                }
                if self.partial {
                    return Err(Code::ListFailed);
                }
                self.close_child();
                self.next();
            }
            Operation::Open => {
                self.op = if self.world.node(self.current.id)?.directory {
                    Operation::List
                } else {
                    Operation::Read
                };
                self.phase = Phase::Ready;
            }
            Operation::Read => {
                self.charge(Resource::Evidence, self.chunk_len as u64)?;
                let end = self.e.len.checked_add(self.chunk_len).ok_or(Code::Budget)?;
                if end > self.e.bytes.len() {
                    return Err(Code::Budget);
                }
                self.e.bytes[self.e.len..end].copy_from_slice(&self.chunk[..self.chunk_len]);
                self.e.len = end;
                self.chunk.fill(0);
                self.chunk_len = 0;
                self.close_child();
                self.next();
            }
        }
        Ok(())
    }
    fn next(&mut self) {
        if self.head < self.tail {
            self.current = self.queue[self.head]
                .take()
                .expect("bounded admitted queue");
            self.head += 1;
            self.op = Operation::Open;
            self.phase = Phase::Ready;
        } else {
            self.stop(if self.e.admitted == 0 {
                Code::CompleteEmpty
            } else {
                Code::Complete
            });
        }
    }
}
