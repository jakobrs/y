use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use anyhow::Result;
use clap::Parser;
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
use rodio::{OutputStream, Source};
use vst::{
    api::{Event, EventType, Events, MidiEvent},
    host::{Host, HostBuffer, PluginInstance, PluginLoader},
    plugin::Plugin,
};
use winit::{
    event_loop::{ControlFlow, EventLoop},
    window::Window,
};

#[derive(Parser)]
struct Args {
    path: PathBuf,

    #[clap(long)]
    disable_editor: bool,
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

/// An iterator over the samples produced by a plugin
struct PluginSource {
    plugin: PluginInstance,
    host_buffer: HostBuffer<f32>,
    inputs: Vec<Vec<f32>>,
    outputs: Vec<Vec<f32>>,

    current_position: usize,
    current_channel: usize,

    length: usize,
    channels: usize,
}

unsafe impl Send for PluginSource {}

impl Iterator for PluginSource {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_position == self.length {
            let mut audio_buffer = self.host_buffer.bind(&self.inputs, &mut self.outputs);
            self.plugin.process(&mut audio_buffer);
            self.current_position = 0;
        }

        let result = self.outputs[self.current_channel][self.current_position];

        self.current_channel += 1;
        if self.current_channel == self.channels {
            self.current_channel = 0;
            self.current_position += 1;
        }

        Some(result)
    }
}
impl Source for PluginSource {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> u16 {
        self.channels as u16
    }

    fn sample_rate(&self) -> u32 {
        44_100
    }

    fn total_duration(&self) -> Option<std::time::Duration> {
        None
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

    let editor = plugin.get_editor();

    let host_buffer = HostBuffer::from_info(&plugin_info);

    let inputs = vec![vec![1.; 1024]; plugin_info.inputs as usize];
    let outputs = vec![vec![0.; 1024]; plugin_info.outputs as usize];

    // Send a midi signal
    // send_midi_thing(&mut plugin, args.note);

    let (_stream, stream_handle) = OutputStream::try_default()?;
    let source = PluginSource {
        plugin,
        host_buffer,
        inputs,
        outputs,

        current_position: 0,
        current_channel: 0,

        length: 1024,
        channels: 2,
    };
    stream_handle.play_raw(source)?;

    if !args.disable_editor {
        if let Some(mut editor) = editor {
            let event_loop = EventLoop::new();
            let window = Window::new(&event_loop)?;
            let raw_window_handle = window.raw_window_handle();
            let hwnd = match raw_window_handle {
                RawWindowHandle::Win32(win32_handle) => win32_handle.hwnd,
                _ => panic!("unsupported raw handle type: {:?}", raw_window_handle),
            };
            let success = editor.open(hwnd);

            println!("Successfully created window for editor: {}", success);

            event_loop.run(move |event, elwt, control_flow| {
                eprintln!("{event:?}, {elwt:?}");
                *control_flow = ControlFlow::Wait;
            })
        }
    }

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
