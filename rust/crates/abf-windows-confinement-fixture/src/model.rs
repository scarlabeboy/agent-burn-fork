use std::{
    fmt,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
static WORLD_TOKEN: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Code {
    Running,
    CompleteEmpty,
    Complete,
    NoWork,
    Unauthorized,
    BindingMismatch,
    Missing,
    Denied,
    Unsupported,
    Ambiguous,
    Alias,
    Reparse,
    MultiLink,
    WrongVolume,
    ListFailed,
    OpenFailed,
    ReadFailed,
    Budget,
    Allocation,
    CancelPending,
    Cancelled,
    InvalidStep,
}
impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Code {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    Graph,
    Edges,
    Depth,
    Examined,
    Rejected,
    Operations,
    Retries,
    Sources,
    Requested,
    Returned,
    Evidence,
    Handles,
    Steps,
}
impl Resource {
    const fn index(self) -> usize {
        self as usize
    }
}
const CAPS: [u64; 13] = [8, 16, 3, 16, 16, 32, 2, 8, 64, 64, 64, 4, 96];
#[derive(Clone, Copy, Debug)]
pub struct Limits([u64; 13]);
impl Default for Limits {
    fn default() -> Self {
        Self(CAPS)
    }
}
impl Limits {
    pub fn with(mut self, r: Resource, cap: u64) -> Result<Self, Code> {
        if cap > CAPS[r.index()] {
            return Err(Code::Budget);
        }
        self.0[r.index()] = cap;
        Ok(self)
    }
    pub fn ceiling(r: Resource) -> u64 {
        CAPS[r.index()]
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ledger([u64; 13]);
impl Ledger {
    pub fn get(&self, r: Resource) -> u64 {
        self.0[r.index()]
    }
    pub fn counts(&self) -> &[u64; 13] {
        &self.0
    }
    pub(crate) fn charge(&mut self, limits: Limits, r: Resource, n: u64) -> Result<(), Code> {
        let v = self.get(r).checked_add(n).ok_or(Code::Budget)?;
        if v > limits.0[r.index()] {
            return Err(Code::Budget);
        }
        self.0[r.index()] = v;
        Ok(())
    }
    pub(crate) fn peak(&mut self, limits: Limits, r: Resource, n: u64) -> Result<(), Code> {
        if n > limits.0[r.index()] {
            return Err(Code::Budget);
        }
        self.0[r.index()] = self.get(r).max(n);
        Ok(())
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Environment {
    Windows,
    Wsl,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Binding {
    pub environment: Environment,
    pub descriptor: u64,
    pub revision: u64,
    pub scope: u64,
    pub attempt: u64,
    pub generation: u64,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Object(pub(crate) usize, pub(crate) u64);
#[derive(Clone, Copy, Debug)]
pub enum Fault {
    Missing,
    Denied,
    Unsupported,
    Ambiguous,
    Moved,
    Alias,
    Reparse,
    MultiLink,
    UnknownLinks,
    WrongVolume,
}
#[derive(Clone, Copy)]
pub(crate) struct Node {
    pub parent: Option<Object>,
    pub directory: bool,
    pub children: [Option<Object>; 4],
    pub len: usize,
    pub data: [u8; 8],
    pub bytes: usize,
    pub revision: u64,
    pub fault: Option<Fault>,
    pub volume: u64,
    pub outside: bool,
}
impl Node {
    fn directory(parent: Option<Object>, outside: bool) -> Self {
        Self {
            parent,
            directory: true,
            children: [None; 4],
            len: 0,
            data: [0; 8],
            bytes: 0,
            revision: 0,
            fault: None,
            volume: 1,
            outside,
        }
    }
}
#[derive(Clone)]
pub struct FakeWorld {
    pub(crate) identity: Arc<()>,
    pub(crate) nodes: [Option<Node>; 8],
    pub(crate) len: usize,
    pub(crate) selected: Object,
    pub(crate) token: u64,
}
impl Default for FakeWorld {
    fn default() -> Self {
        Self::new()
    }
}
impl FakeWorld {
    /// The outside sentinel occupies a reserved graph slot and is never approved.
    pub fn new() -> Self {
        let mut nodes = [None; 8];
        nodes[0] = Some(Node::directory(None, false));
        nodes[1] = Some(Node::directory(None, true));
        let token = WORLD_TOKEN
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
            .expect("fictional world identity space exhausted");
        Self {
            identity: Arc::new(()),
            nodes,
            len: 2,
            selected: Object(0, token),
            token,
        }
    }
    pub fn root(&self) -> Object {
        self.selected
    }
    pub fn sentinel(&self) -> Object {
        Object(1, self.token)
    }
    pub(crate) fn node(&self, id: Object) -> Result<Node, Code> {
        if id.1 != self.token {
            return Err(Code::BindingMismatch);
        }
        self.nodes
            .get(id.0)
            .copied()
            .flatten()
            .ok_or(Code::BindingMismatch)
    }
    pub fn directory(&mut self, parent: Object) -> Result<Object, Code> {
        self.add(parent, None)
    }
    pub fn file(&mut self, parent: Object, bytes: &[u8]) -> Result<Object, Code> {
        self.add(parent, Some(bytes))
    }
    fn add(&mut self, parent: Object, bytes: Option<&[u8]>) -> Result<Object, Code> {
        let mut p = self.node(parent)?;
        if !p.directory || p.len == 4 || self.len == 8 || bytes.is_some_and(|b| b.len() > 8) {
            return Err(Code::Budget);
        }
        let mut n = Node::directory(Some(parent), p.outside);
        if let Some(b) = bytes {
            n.directory = false;
            n.data[..b.len()].copy_from_slice(b);
            n.bytes = b.len();
        }
        let id = Object(self.len, self.token);
        p.children[p.len] = Some(id);
        p.len += 1;
        p.revision = p.revision.checked_add(1).ok_or(Code::Budget)?;
        self.nodes[parent.0] = Some(p);
        self.nodes[id.0] = Some(n);
        self.len += 1;
        Ok(id)
    }
    pub fn fault(&mut self, id: Object, fault: Fault) -> Result<(), Code> {
        let mut n = self.node(id)?;
        n.fault = Some(fault);
        self.nodes[id.0] = Some(n);
        Ok(())
    }
    pub fn substitute(
        &mut self,
        parent: Object,
        ordinal: usize,
        target: Object,
    ) -> Result<(), Code> {
        self.node(target)?;
        let mut n = self.node(parent)?;
        if ordinal >= n.len {
            return Err(Code::InvalidStep);
        }
        n.revision = n.revision.checked_add(1).ok_or(Code::Budget)?;
        n.children[ordinal] = Some(target);
        self.nodes[parent.0] = Some(n);
        Ok(())
    }
    pub fn replace_root_with_sentinel(&mut self) {
        self.selected = self.sentinel();
    }
    pub(crate) fn inspect(&self, id: Object, root: Object, volume: u64) -> Result<(), Code> {
        let mut at = id;
        for _ in 0..8 {
            let n = self.node(at)?;
            if n.outside {
                return Err(Code::BindingMismatch);
            }
            if n.volume != volume {
                return Err(Code::WrongVolume);
            }
            if let Some(f) = n.fault {
                return Err(match f {
                    Fault::Missing => Code::Missing,
                    Fault::Denied => Code::Denied,
                    Fault::Unsupported => Code::Unsupported,
                    Fault::Ambiguous => Code::Ambiguous,
                    Fault::Moved => Code::BindingMismatch,
                    Fault::Alias => Code::Alias,
                    Fault::Reparse => Code::Reparse,
                    Fault::MultiLink | Fault::UnknownLinks => Code::MultiLink,
                    Fault::WrongVolume => Code::WrongVolume,
                });
            }
            let incoming = self
                .nodes
                .iter()
                .flatten()
                .flat_map(|p| p.children[..p.len].iter())
                .filter(|v| **v == Some(at))
                .count();
            if at == root {
                if n.parent.is_some() || incoming != 0 {
                    return Err(Code::Alias);
                }
                return Ok(());
            }
            if incoming != 1 {
                return Err(Code::Alias);
            }
            let p = n.parent.ok_or(Code::BindingMismatch)?;
            let parent = self.node(p)?;
            if !parent.children[..parent.len].contains(&Some(at)) {
                return Err(Code::BindingMismatch);
            }
            at = p;
        }
        Err(Code::BindingMismatch)
    }
}
pub struct FictionalAuthority;
pub struct Selection {
    pub(crate) identity: Arc<()>,
    pub(crate) binding: Binding,
    pub(crate) root: Object,
    pub(crate) volume: u64,
    pub(crate) revision: u64,
}
pub struct FakeRoot {
    pub(crate) selection: Selection,
}
impl FictionalAuthority {
    /// Explicit fictional approval only. This authenticates no human or OS.
    pub fn explicit_approval(world: &FakeWorld, binding: Binding) -> Result<Selection, Code> {
        if binding.environment != Environment::Windows {
            return Err(Code::Unauthorized);
        }
        let n = world.node(world.root())?;
        world.inspect(world.root(), world.root(), n.volume)?;
        Ok(Selection {
            identity: world.identity.clone(),
            binding,
            root: world.root(),
            volume: n.volume,
            revision: n.revision,
        })
    }
}
macro_rules! redact {
    ($($t:ty),*) => { $(impl fmt::Debug for $t {
        fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result { f.write_str(concat!(stringify!($t), "([redacted])")) }
    })* };
}
redact!(FakeWorld, Selection, FakeRoot, Object, Binding, Environment);

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_meter_checks_below_at_above_and_overflow() {
        for r in [
            Resource::Graph,
            Resource::Edges,
            Resource::Depth,
            Resource::Examined,
            Resource::Rejected,
            Resource::Operations,
            Resource::Retries,
            Resource::Sources,
            Resource::Requested,
            Resource::Returned,
            Resource::Evidence,
            Resource::Handles,
            Resource::Steps,
        ] {
            let cap = Limits::ceiling(r);
            let mut l = Ledger::default();
            l.charge(Limits::default(), r, cap - 1).unwrap();
            assert_eq!(l.get(r), cap - 1);
            l.charge(Limits::default(), r, 1).unwrap();
            assert_eq!(l.get(r), cap);
            assert_eq!(l.charge(Limits::default(), r, 1), Err(Code::Budget));
            assert_eq!(l.get(r), cap);
            assert!(Limits::default().with(r, cap + 1).is_err());
            l.0[r.index()] = u64::MAX;
            assert_eq!(l.charge(Limits::default(), r, 1), Err(Code::Budget));
            assert_eq!(l.get(r), u64::MAX);
        }
    }
}
