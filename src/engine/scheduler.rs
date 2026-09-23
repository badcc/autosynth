use crate::music::phrase::{Event, Locks};

/// A note event resolved to a global beat position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NoteEv {
    On { note: u8, vel: f32, slide: bool, locks: Locks },
    Off { note: u8 },
}

/// A note-on and the global beat at which it fires, paired with the resolved
/// global beat of its release obligation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fired {
    pub beat: f64,
    pub ev: NoteEv,
}

/// A pattern swap queued to apply at the next loop boundary. `choke` mirrors the
/// old `all_notes_off` behaviour: it is set only when the *pattern was replaced*
/// (a real edit), never as a leak-plugging default.
pub struct Queued {
    pub ons: Vec<Event>,
    pub loop_len: Option<f64>,
    pub choke: bool,
}

/// Per-track beat-native scheduler.
///
/// Note-ons live in a sorted list keyed by loop-local beat; each fired note-on
/// registers its note-off as an *obligation* at an absolute global beat, so
/// releases scheduled past the loop boundary survive the wrap instead of being
/// dropped (the old bug). One-shot (non-looping) tracks simply play the list
/// once.
pub struct Scheduler {
    ons: Vec<Event>,
    loop_len: Option<f64>,
    /// Global beat of the current iteration's loop-local zero.
    iter_base: f64,
    cursor: usize,
    iteration: u32,
    active: bool,
    /// Outstanding note-offs at absolute global beats.
    pending_offs: Vec<(f64, u8)>,
    queued: Option<Queued>,
}

impl Scheduler {
    pub fn new() -> Self {
        Self {
            ons: Vec::new(),
            loop_len: None,
            iter_base: 0.0,
            cursor: 0,
            iteration: 0,
            active: false,
            pending_offs: Vec::new(),
            queued: None,
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn is_looping(&self) -> bool {
        self.active && self.loop_len.is_some()
    }

    pub fn iteration(&self) -> u32 {
        self.iteration
    }

    pub fn has_queued(&self) -> bool {
        self.queued.is_some()
    }

    /// Start (or restart) playback at global beat `now`. Sorted note-ons and an
    /// optional loop length define the pattern. Clears any prior state.
    pub fn launch(&mut self, mut ons: Vec<Event>, loop_len: Option<f64>, now: f64) {
        ons.sort_by(|a, b| a.beat.total_cmp(&b.beat));
        self.ons = ons;
        self.loop_len = loop_len.filter(|l| *l > 0.0);
        self.iter_base = now;
        self.cursor = 0;
        self.iteration = 0;
        self.active = true;
        self.pending_offs.clear();
        self.queued = None;
    }

    /// Queue a pattern/loop swap for the next loop boundary. Non-looping tracks
    /// apply it immediately from `now`.
    pub fn queue(&mut self, ons: Vec<Event>, loop_len: Option<f64>, choke: bool, now: f64) {
        if self.is_looping() {
            self.queued = Some(Queued {
                ons,
                loop_len: loop_len.filter(|l| *l > 0.0),
                choke,
            });
        } else {
            // Nothing to wait for — relaunch now.
            self.launch(ons, loop_len, now);
        }
    }

    pub fn stop(&mut self) {
        self.active = false;
    }

    /// The global beat of the next loop boundary, if looping. The track uses
    /// this to regenerate the upcoming iteration's phrase just before the wrap.
    pub fn next_boundary(&self) -> Option<f64> {
        if self.is_looping() {
            Some(self.iter_base + self.loop_len?)
        } else {
            None
        }
    }

    /// Global beat at which the current iteration began.
    pub fn iter_base(&self) -> f64 {
        self.iter_base
    }

    /// Collect every note event that fires in the beat range `[b0, b1)` into
    /// `out`, resolving loop wraps and note-off obligations. Results are sorted
    /// by beat. `beats_per` maps a note's beat-duration into the same beat units
    /// (always 1.0 here since durations are already beats).
    pub fn collect(&mut self, b0: f64, b1: f64, out: &mut Vec<Fired>) {
        if !self.active {
            return;
        }

        // Emit due note-off obligations.
        let mut i = 0;
        while i < self.pending_offs.len() {
            let (beat, note) = self.pending_offs[i];
            if beat < b1 {
                out.push(Fired {
                    beat: beat.max(b0),
                    ev: NoteEv::Off { note },
                });
                self.pending_offs.swap_remove(i);
            } else {
                i += 1;
            }
        }

        // Emit note-ons, wrapping loops as needed.
        loop {
            let loop_len = self.loop_len;
            if self.cursor >= self.ons.len() {
                match loop_len {
                    Some(l) => {
                        let boundary = self.iter_base + l;
                        if boundary >= b1 {
                            break;
                        }
                        self.wrap(boundary, out);
                        continue;
                    }
                    None => {
                        self.active = false;
                        break;
                    }
                }
            }

            let spec = self.ons[self.cursor];
            let g = self.iter_base + spec.beat as f64;
            if g >= b1 {
                break;
            }
            out.push(Fired {
                beat: g,
                ev: NoteEv::On {
                    note: spec.note,
                    vel: spec.vel,
                    slide: spec.slide,
                    locks: spec.locks,
                },
            });
            self.pending_offs.push((g + spec.dur as f64, spec.note));
            self.cursor += 1;
        }

        out.sort_by(|a, b| a.beat.total_cmp(&b.beat));
    }

    /// Advance to the next loop iteration at `boundary`, applying any queued
    /// swap (and choking held notes if the pattern was replaced). With no queued
    /// swap the same note-on list repeats.
    fn wrap(&mut self, boundary: f64, out: &mut Vec<Fired>) {
        self.iteration += 1;
        self.cursor = 0;
        self.iter_base = boundary;

        if let Some(q) = self.queued.take() {
            if q.choke {
                for (_, note) in self.pending_offs.drain(..) {
                    out.push(Fired {
                        beat: boundary,
                        ev: NoteEv::Off { note },
                    });
                }
            }
            self.ons = q.ons;
            self.ons
                .sort_by(|a, b| a.beat.total_cmp(&b.beat));
            // Only a loop-length change restarts the iteration count — a plain
            // regeneration keeps it climbing.
            if q.loop_len != self.loop_len {
                self.loop_len = q.loop_len;
                self.iteration = 0;
            }
        }
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}
