use midir::{MidiInput, MidiInputConnection};

use crate::engine::command::{Command, EngineHandle};

/// Connect to the first MIDI input port. Notes go to the track `s.midi(..)`
/// armed; control changes become `knob(cc)` signals.
#[cfg_attr(not(feature = "hot-reload"), allow(dead_code))]
pub(crate) fn connect(handle: EngineHandle) -> Option<MidiInputConnection<()>> {
    let midi_in = MidiInput::new("autosynth").ok()?;
    let ports = midi_in.ports();
    let port = ports.first()?;
    tracing::info!("MIDI: connecting to {}", midi_in.port_name(port).unwrap_or_default());
    midi_in
        .connect(
            port,
            "autosynth-in",
            move |_stamp, message, _| {
                if message.len() < 3 {
                    return;
                }
                let (status, a, b) = (message[0] & 0xF0, message[1], message[2]);
                match status {
                    0x90 if b > 0 => handle.send(Command::MidiNoteOn { note: a, vel: b as f32 / 127.0 }),
                    0x90 | 0x80 => handle.send(Command::MidiNoteOff { note: a }),
                    0xB0 => handle.send(Command::MidiCc { cc: a, value: b as f32 / 127.0 }),
                    _ => {}
                }
            },
            (),
        )
        .ok()
}
