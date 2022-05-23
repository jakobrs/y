use std::{
    ops::{Deref, DerefMut},
    path::PathBuf,
    sync::{Arc, Mutex},
};

use anyhow::Result;
use clap::Parser;
use jack::{AudioIn, AudioOut};
use vst::{
    api::{Event, EventType, Events, MidiEvent},
    host::{Host, HostBuffer, PluginInstance, PluginLoader},
    plugin::Plugin,
};

#[derive(Parser)]
struct Args {
    path: PathBuf,
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

    let callback = move |_client: &jack::Client, ps: &jack::ProcessScope| -> jack::Control {
        // it's probably a bad idea to re-allocate these two vectors on every call but who cares
        let inputs: Vec<&[f32]> = input_ports.iter().map(|port| port.as_slice(ps)).collect();
        let mut outputs: Vec<&mut [f32]> = output_ports
            .iter_mut()
            .map(|port| port.as_mut_slice(ps))
            .collect();

        let mut audio_buffer = host_buffer.bind(&inputs, &mut outputs);
        plugin.process(&mut audio_buffer);

        jack::Control::Continue
    };

    let _async_client = client.activate_async((), jack::ClosureProcessHandler::new(callback))?;

    std::io::stdin().read_line(&mut String::new())?;

    Ok(())
}

/// Sends a midi on event on channel 0 with velocity 0x7f
#[allow(dead_code)]
fn send_midi(plugin: &mut PluginInstance, data: [u8; 3]) {
    let num_events = 1;
    let _reserved = 0;
    let mut event_0 = MidiEvent {
        event_type: EventType::Midi,
        byte_size: std::mem::size_of::<MidiEvent>() as i32,
        delta_frames: 0,
        flags: 0,
        note_length: 0,
        note_offset: 0,
        midi_data: data,
        _midi_reserved: 0,
        detune: 0,
        note_off_velocity: 0,
        _reserved1: 0,
        _reserved2: 0,
    };
    let events = [&mut event_0 as *mut _ as *mut Event, std::ptr::null_mut()];
    let events_struct = Events {
        num_events,
        _reserved,
        events,
    };

    plugin.process_events(&events_struct);
}
