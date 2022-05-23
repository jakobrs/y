use std::{
    ops::{Deref, DerefMut},
    path::PathBuf,
    sync::{Arc, Mutex},
};

use anyhow::Result;
use clap::Parser;
use jack::{AudioIn, AudioOut, MidiIn, RawMidi};
use vst::{
    api::{EventType, Events, MidiEvent},
    host::{Host, HostBuffer, PluginInstance, PluginLoader},
    plugin::Plugin,
};

#[derive(Parser)]
struct Args {
    path: PathBuf,

    #[clap(long)]
    extra_midi_in: i32,
}

struct MyHost;

impl Host for MyHost {
    fn automate(&self, index: i32, value: f32) {
        println!("{index} {value}");
    }

    fn process_events(&self, events: &vst::api::Events) {
        println!("{:?}", events.num_events);
    }

    fn update_display(&self) {
        println!("update_display called");
    }
}

struct SendHostBuffer(HostBuffer<f32>);
unsafe impl Send for SendHostBuffer {}

impl Deref for SendHostBuffer {
    type Target = HostBuffer<f32>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for SendHostBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    let host = Arc::new(Mutex::new(MyHost));

    // load the plugin
    let mut plugin_loader = PluginLoader::load(&args.path, host)?;
    let mut plugin = plugin_loader.instance()?;

    let plugin_info = plugin.get_info();

    // initialise the plugin
    plugin.init();

    let mut host_buffer = SendHostBuffer(HostBuffer::from_info(&plugin_info));

    let (client, _client_status) =
        jack::Client::new("vst-host", jack::ClientOptions::NO_START_SERVER)?;

    // setup ports
    let input_ports: Vec<jack::Port<AudioIn>> = (0..plugin_info.inputs)
        .map(|i| client.register_port(&format!("in{i}"), AudioIn::default()))
        .collect::<Result<_, _>>()?;
    let mut output_ports: Vec<jack::Port<AudioOut>> = (0..plugin_info.outputs)
        .map(|i| client.register_port(&format!("out{i}"), AudioOut::default()))
        .collect::<Result<_, _>>()?;

    let midi_input_ports: Vec<jack::Port<MidiIn>> = (0..plugin_info.midi_inputs
        + args.extra_midi_in as i32)
        .map(|i| client.register_port(&format!("midi_in{i}"), MidiIn::default()))
        .collect::<Result<_, _>>()?;

    let mut midi_events = vec![];

    let callback = move |_client: &jack::Client, ps: &jack::ProcessScope| -> jack::Control {
        // it's probably a bad idea to re-allocate these two vectors on every call but who cares
        let inputs: Vec<&[f32]> = input_ports.iter().map(|port| port.as_slice(ps)).collect();
        let mut outputs: Vec<&mut [f32]> = output_ports
            .iter_mut()
            .map(|port| port.as_mut_slice(ps))
            .collect();

        midi_events.clear();
        for port in midi_input_ports.iter() {
            for raw_midi in port.iter(ps) {
                midi_events.push(midi_event_from_raw_midi(raw_midi));
            }
        }

        send_midi(&mut plugin, midi_events.as_slice());

        let mut audio_buffer = host_buffer.bind(&inputs, &mut outputs);
        plugin.process(&mut audio_buffer);

        jack::Control::Continue
    };

    let _async_client = client.activate_async((), jack::ClosureProcessHandler::new(callback))?;

    std::io::stdin().read_line(&mut String::new())?;

    Ok(())
}

fn midi_event_from_raw_midi(raw_midi: RawMidi) -> MidiEvent {
    let mut midi_data = [0, 0, 0];
    midi_data[..raw_midi.bytes.len()].copy_from_slice(raw_midi.bytes);

    let _reserved = 0;

    MidiEvent {
        event_type: EventType::Midi,
        byte_size: std::mem::size_of::<MidiEvent>() as i32,
        delta_frames: 0,
        flags: 0,
        note_length: 0,
        note_offset: raw_midi.time as i32,
        midi_data,
        _midi_reserved: 0,
        detune: 0,
        note_off_velocity: 0,
        _reserved1: 0,
        _reserved2: 0,
    }
}

#[allow(dead_code)]
fn send_midi(plugin: &mut PluginInstance, midi_events: &[MidiEvent]) {
    let num_events = midi_events.len();
    let _reserved = 0;

    let a: Vec<u64> = [u64::from_le(num_events as u64), 0]
        .into_iter()
        .chain(midi_events.iter().map(|event| event as *const _ as u64))
        .collect();

    // SAFETY: none
    let events: &Events = unsafe { std::mem::transmute(a.as_slice().as_ptr()) };

    plugin.process_events(events);
}
