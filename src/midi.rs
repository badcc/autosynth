use midir::{MidiInput, MidiInputConnection};

use crate::engine::EngineHandle;

pub(crate) fn connect(handle: EngineHandle) -> Option<MidiInputConnection<()>> {
    let midi_in = MidiInput::new("autosynth").ok()?;
    let ports = midi_in.ports();
    let port = ports.first()?;
    let port_name = midi_in.port_name(port).unwrap_or_default();
    tracing::info!("MIDI: connecting to {port_name}");

    midi_in
        .connect(
            port,
            "autosynth-in",
            move |_stamp, message, _| {
                if message.len() >= 3 {
                    let status = message[0] & 0xF0;
                    let note = message[1];
                    let vel = message[2];
                    match status {
                        0x90 if vel > 0 => handle.midi_note_on(note, vel as f32 / 127.0),
                        0x90 | 0x80 => handle.midi_note_off(note),
                        _ => {}
                    }
                }
            },
            (),
        )
        .ok()
}
