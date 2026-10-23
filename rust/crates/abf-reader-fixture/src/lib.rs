//! Pure synthetic S1 emission. See the fixture-local README contract.
#![forbid(unsafe_code)]
use abf_source_run::{
    AttemptRef, CatalogScope, CatalogSnapshot, ChangeObservation, OpenFailure, PartialReason,
    Payload, RawEnumeration, ReaderFrame, SourceEvent, SourceRef, StopReason,
};
pub const MAX_STEPS: usize = 512;
pub const MAX_RECORDS: usize = 128;
pub const MAX_BYTES: usize = 131072;
pub use abf_source_run::MAX_RECORD_BYTES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidScript,
    BudgetExceeded,
    ResourceExhausted,
}
#[derive(Clone, Copy, Debug)]
pub enum Action {
    OpenAttempt,
    OpenSuccess,
    OpenFail(OpenFailure),
    Vanished,
    Skip(StopReason),
    End(ChangeObservation),
    ReadError(PartialReason),
}
#[derive(Clone, Copy)]
pub enum Event<'a> {
    Enumeration {
        outcome: RawEnumeration,
        sources: &'a [SourceRef],
    },
    Source {
        source: &'a SourceRef,
        source_seq: u64,
        action: Action,
    },
    Record {
        source: &'a SourceRef,
        source_seq: u64,
        bytes: &'a [u8],
    },
    Replay {
        original: usize,
        replacement: Option<&'a [u8]>,
    },
    Cancel,
    RunEnd,
}
#[derive(Clone, Copy)]
pub struct Step<'a> {
    pub frame_seq: u64,
    pub event: Event<'a>,
}
pub struct Emission {
    frames: Vec<ReaderFrame>,
    cancelled: bool,
}
impl Emission {
    pub fn frames(&self) -> &[ReaderFrame] {
        &self.frames
    }
    pub fn into_frames(self) -> Vec<ReaderFrame> {
        self.frames
    }
    pub fn cancelled(&self) -> bool {
        self.cancelled
    }
}
macro_rules! redact {($($t:ty),*)=>{$(impl std::fmt::Debug for $t {
    fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {f.write_str("[redacted]")}
})*};}
redact!(Event<'_>, Step<'_>, Emission);

fn add(n: usize, amount: usize, limit: usize) -> Result<usize, Error> {
    n.checked_add(amount)
        .filter(|v| *v <= limit)
        .ok_or(Error::BudgetExceeded)
}
fn record_bytes<'a>(steps: &[Step<'a>], index: usize) -> Result<Option<&'a [u8]>, Error> {
    match steps[index].event {
        Event::Record { bytes, .. } => Ok(Some(bytes)),
        Event::Replay {
            original,
            replacement,
        } => {
            if original >= index
                || matches!(steps[original].event, Event::Replay { .. } | Event::Cancel)
            {
                return Err(Error::InvalidScript);
            }
            match steps[original].event {
                Event::Record { bytes, .. } => Ok(Some(replacement.unwrap_or(bytes))),
                _ if replacement.is_none() => Ok(None),
                _ => Err(Error::InvalidScript),
            }
        }
        _ => Ok(None),
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Unopened,
    Pending,
    Open,
    Done,
}
#[derive(Clone, Copy)]
struct Progress {
    state: State,
    records: u64,
}
const START: Progress = Progress {
    state: State::Unopened,
    records: 0,
};

// No transformed output or parser allocation occurs during this full preflight.
fn validate(
    catalog: &CatalogSnapshot,
    scope: &CatalogScope,
    steps: &[Step<'_>],
) -> Result<(usize, bool), Error> {
    if catalog.scope_tag(scope).is_none() {
        return Err(Error::InvalidScript);
    }
    if steps.len() > MAX_STEPS {
        return Err(Error::BudgetExceeded);
    }
    let (mut bytes, mut records) = (0, 0);
    for i in 0..steps.len() {
        if let Some(b) = record_bytes(steps, i)? {
            if b.len() > MAX_RECORD_BYTES {
                return Err(Error::BudgetExceeded);
            }
            bytes = add(bytes, b.len(), MAX_BYTES)?;
            records = add(records, 1, MAX_RECORDS)?;
        }
        if let Event::Enumeration { sources, .. } = steps[i].event
            && sources.len() > abf_source_run::MAX_INSTANCES_PER_SCOPE as usize
        {
            return Err(Error::BudgetExceeded);
        }
    }
    let mut expected: Option<&[SourceRef]> = None;
    let mut progress = [START; abf_source_run::MAX_INSTANCES_PER_SCOPE as usize];
    let mut ended = false;
    for (i, step) in steps.iter().enumerate() {
        if matches!(step.event, Event::Cancel) {
            if ended {
                return Err(Error::InvalidScript);
            }
            return Ok((i, true));
        }
        if matches!(step.event, Event::Replay { .. }) {
            continue;
        }
        if ended
            || step.frame_seq == 0
            || u32::try_from(step.frame_seq).is_err()
            || steps[..i]
                .iter()
                .any(|s| !matches!(s.event, Event::Replay { .. }) && s.frame_seq == step.frame_seq)
        {
            return Err(Error::InvalidScript);
        }
        match step.event {
            Event::Enumeration { outcome, sources } => {
                if expected.is_some() {
                    return Err(Error::InvalidScript);
                }
                let count = u64::try_from(sources.len()).map_err(|_| Error::BudgetExceeded)?;
                match outcome {
                    RawEnumeration::Closed(n) | RawEnumeration::Partial(n, _) if n == count => {}
                    RawEnumeration::Closed(_) | RawEnumeration::Partial(_, _) => {
                        return Err(Error::InvalidScript);
                    }
                    _ if !sources.is_empty() => return Err(Error::InvalidScript),
                    _ => {}
                }
                let tag = catalog.scope_tag(scope).ok_or(Error::InvalidScript)?;
                for (j, s) in sources.iter().enumerate() {
                    if s.as_str().split_once('-').map(|p| p.0) != Some(tag)
                        || sources[..j].contains(s)
                    {
                        return Err(Error::InvalidScript);
                    }
                }
                expected = Some(sources);
            }
            Event::Source {
                source, source_seq, ..
            }
            | Event::Record {
                source, source_seq, ..
            } => {
                let j = expected
                    .ok_or(Error::InvalidScript)?
                    .iter()
                    .position(|s| s == source)
                    .ok_or(Error::InvalidScript)?;
                let p = &mut progress[j];
                if source_seq == 0 || u32::try_from(source_seq).is_err() {
                    return Err(Error::InvalidScript);
                }
                match step.event {
                    Event::Source { action, .. } => {
                        p.state = match (p.state, action) {
                            (State::Unopened, Action::OpenAttempt) => State::Pending,
                            (State::Pending, Action::OpenSuccess) => State::Open,
                            (State::Pending, Action::OpenFail(_) | Action::Vanished)
                            | (State::Unopened, Action::Skip(_))
                            | (State::Open, Action::End(_) | Action::ReadError(_)) => State::Done,
                            _ => return Err(Error::InvalidScript),
                        }
                    }
                    Event::Record { .. }
                        if p.state == State::Open
                            && source_seq
                                == p.records.checked_add(1).ok_or(Error::BudgetExceeded)? =>
                    {
                        p.records = p.records.checked_add(1).ok_or(Error::BudgetExceeded)?;
                    }
                    _ => return Err(Error::InvalidScript),
                }
            }
            Event::RunEnd => {
                let list = expected.ok_or(Error::InvalidScript)?;
                if progress[..list.len()]
                    .iter()
                    .any(|p| p.state != State::Done)
                {
                    return Err(Error::InvalidScript);
                }
                ended = true;
            }
            Event::Replay { .. } | Event::Cancel => return Err(Error::InvalidScript),
        }
    }
    Ok((steps.len(), false))
}
fn reserve<T>(v: &mut Vec<T>, amount: usize) -> Result<(), Error> {
    v.try_reserve_exact(amount)
        .map_err(|_| Error::ResourceExhausted)
}
fn copy_bytes(bytes: &[u8]) -> Result<Vec<u8>, Error> {
    let mut v = Vec::new();
    reserve(&mut v, bytes.len())?;
    v.extend_from_slice(bytes);
    Ok(v)
}
fn text(s: &str) -> Result<String, Error> {
    let mut v = String::new();
    v.try_reserve_exact(s.len())
        .map_err(|_| Error::ResourceExhausted)?;
    v.push_str(s);
    Ok(v)
}
fn payload(step: &Step<'_>, replacement: Option<&[u8]>, records: u64) -> Result<Payload, Error> {
    let source_event = match step.event {
        Event::Enumeration { outcome, sources } => {
            let mut names = Vec::new();
            reserve(&mut names, sources.len())?;
            for s in sources {
                names.push(text(s.as_str())?);
            }
            return Ok(Payload::Enumeration {
                member: 0,
                second: false,
                outcome,
                sources: names,
                observed_at: None,
            });
        }
        Event::RunEnd => return Ok(Payload::RunEnd),
        Event::Record { bytes, .. } => {
            let bytes = replacement.unwrap_or(bytes);
            SourceEvent::Record {
                bytes: copy_bytes(bytes)?,
                result: abf_parser::parse(bytes, None, abf_parser::Limits::MAX),
            }
        }
        Event::Source { action, .. } => match action {
            Action::OpenAttempt => SourceEvent::OpenAttempt,
            Action::OpenSuccess => SourceEvent::OpenSuccess,
            Action::OpenFail(r) => SourceEvent::OpenFailed(r),
            Action::Vanished => SourceEvent::Vanished,
            Action::Skip(r) => SourceEvent::Skipped(r),
            Action::End(change) => SourceEvent::End { records, change },
            Action::ReadError(r) => SourceEvent::ReadError(r),
        },
        _ => return Err(Error::InvalidScript),
    };
    match step.event {
        Event::Record {
            source, source_seq, ..
        }
        | Event::Source {
            source, source_seq, ..
        } => Ok(Payload::Source {
            source: text(source.as_str())?,
            source_seq,
            event: source_event,
        }),
        _ => Err(Error::InvalidScript),
    }
}
/// Atomic, bounded emission of fictional reader facts only. No supervisor facts.
pub fn emit(
    attempt: &AttemptRef,
    catalog: &CatalogSnapshot,
    scope: &CatalogScope,
    steps: &[Step<'_>],
) -> Result<Emission, Error> {
    let (length, cancelled) = validate(catalog, scope, steps)?;
    let mut frames = Vec::new();
    reserve(&mut frames, length)?;
    for (i, step) in steps[..length].iter().enumerate() {
        let (original, replacement) = match step.event {
            Event::Replay {
                original,
                replacement,
            } => (original, replacement),
            _ => (i, None),
        };
        let base = &steps[original];
        let records = match base.event {
            Event::Source { source, .. } => u64::try_from(
                steps[..original]
                    .iter()
                    .filter(|s| matches!(s.event,Event::Record{source:r,..} if r==source))
                    .count(),
            )
            .map_err(|_| Error::BudgetExceeded)?,
            _ => 0,
        };
        frames.push(ReaderFrame {
            attempt: attempt.clone(),
            frame_seq: base.frame_seq,
            payload: payload(base, replacement, records)?,
        });
    }
    Ok(Emission { frames, cancelled })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checked_resource_failure_and_overflow_are_fixed_errors() {
        assert_eq!(add(usize::MAX, 1, usize::MAX), Err(Error::BudgetExceeded));
        let mut v: Vec<u8> = Vec::new();
        assert_eq!(reserve(&mut v, usize::MAX), Err(Error::ResourceExhausted));
        assert!(v.is_empty());
        assert_eq!(MAX_RECORD_BYTES, abf_parser::Limits::MAX.frame_bytes);
        assert_eq!(MAX_STEPS.min(abf_source_run::MAX_FRAMES), MAX_STEPS);
    }
}
