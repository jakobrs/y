use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use clap::Parser;
use vst::{
    host::{Host, PluginLoader},
    plugin::{Plugin, PluginParameters, CanDo},
};

#[derive(Parser)]
struct Args {
    path: PathBuf,
}

struct MyHost;

impl Host for MyHost {}

type Result<T, E = Box<dyn std::error::Error>> = std::result::Result<T, E>;

const CAN_DO: [CanDo; 11] = [
    CanDo::Bypass,
    CanDo::MidiKeyBasedInstrumentControl,
    CanDo::MidiProgramNames,
    CanDo::MidiSingleNoteTuningChange,
    CanDo::Offline,
    CanDo::ReceiveEvents,
    CanDo::ReceiveMidiEvent,
    CanDo::ReceiveSysExEvent,
    CanDo::ReceiveTimeInfo,
    CanDo::SendEvents,
    CanDo::SendMidiEvent
];

fn main() -> Result<()> {
    let args = Args::parse();

    let host = Arc::new(Mutex::new(MyHost));

    // load the plugin
    let mut plugin_loader = PluginLoader::load(&args.path, host)?;
    let mut plugin = plugin_loader.instance()?;

    let plugin_info = plugin.get_info();
    println!("{plugin_info:#?}");

    if plugin_info.parameters > 0 {
        println!("Parameters:");
        let parameter_object = plugin.get_parameter_object();
        enumerate_parameters(&*parameter_object, plugin_info.parameters);
    }

    println!("Abilities:");
    for ability in CAN_DO {
        let description = format!("{ability:?}");
        match plugin.can_do(ability) {
            vst::api::Supported::Yes => println!("    {description}: Yes"),
            vst::api::Supported::Maybe => println!("    {description}: Maybe"),
            vst::api::Supported::No => (),
            vst::api::Supported::Custom(custom) => println!("    {description}: Custom {custom}"),
        }
    }

    Ok(())
}

fn enumerate_parameters(parameters: &(impl PluginParameters + ?Sized), parameter_count: i32) {
    for i in 0..parameter_count {
        let name = parameters.get_parameter_name(i);
        let text = parameters.get_parameter_text(i);
        let label = parameters.get_parameter_label(i);
        let value = parameters.get_parameter(i);

        if label == "" {
            println!("    {name} = {text} ({value})");
        } else {
            println!("    {name} = {text} {label} ({value})");
        }
    }
}
