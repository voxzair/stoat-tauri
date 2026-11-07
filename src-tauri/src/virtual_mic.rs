//! PipeWire virtual sink/source for screen-share audio on Linux Wayland.
//!
//! Ported from `node-pipewire` (the native module used by the Electron build).
//! It creates a virtual null sink ("stoat-virtual-sink") and a virtual source
//! ("stoat-virtual-source"), links the sink's monitor into the source, and then
//! periodically links every new output stream from other apps into the sink.

use std::{
    cell::RefCell,
    collections::HashMap,
    rc::Rc,
    sync::{Arc, Mutex},
    time::Duration,
};

use pipewire::{
    context::ContextRc,
    core::Core,
    link::Link,
    main_loop::MainLoopRc,
    node::Node,
    properties::properties,
    proxy::{Proxy, ProxyListener, ProxyT},
    registry::GlobalObject,
    spa::utils::dict::DictRef,
    types::ObjectType,
};

const SINK_NAME: &str = "stoat-virtual-sink";
const SOURCE_NAME: &str = "stoat-virtual-source";

// ---------------------------------------------------------------------------
// Data model (mirrors node-pipewire's PipewireData)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct Port {
    id: u32,
    node_id: u32,
    name: String,
    direction: String,
}

#[derive(Clone, Debug)]
struct NodeInfo {
    id: u32,
    name: String,
    props: HashMap<String, String>,
    ports: Vec<Port>,
}

#[derive(Clone, Debug)]
struct ClientInfo {
    id: u32,
    pid: u32,
    application_name: String,
}

#[derive(Clone, Debug)]
enum Data {
    Node(NodeInfo),
    Port(Port),
    Client(ClientInfo),
}

static ALL_DATA: std::sync::LazyLock<Arc<Mutex<HashMap<u32, Data>>>> =
    std::sync::LazyLock::new(|| Arc::new(Mutex::new(HashMap::new())));

// ---------------------------------------------------------------------------
// Commands sent to the PipeWire thread
// ---------------------------------------------------------------------------

enum Cmd {
    CreateSink {
        name: String,
        position: String,
        channels: u32,
        permanent: bool,
    },
    CreateSource {
        name: String,
        position: String,
        channels: u32,
        permanent: bool,
    },
    LinkNodesNameToId {
        output_nodes_name: String,
        input_node_id: u32,
        permanent: bool,
    },
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub fn init(app_name: String) {
    if !crate::window::is_wayland() {
        return;
    }

    let (sender, receiver) = pipewire::channel::channel::<Cmd>();

    std::thread::spawn(move || pw_thread(receiver));

    // Give the thread a moment to spin up before sending commands.
    std::thread::sleep(Duration::from_millis(100));

    let _ = sender.send(Cmd::CreateSink {
        name: SINK_NAME.to_string(),
        position: "FL,FR".to_string(),
        channels: 2,
        permanent: false,
    });
    let _ = sender.send(Cmd::CreateSource {
        name: SOURCE_NAME.to_string(),
        position: "FL,FR".to_string(),
        channels: 2,
        permanent: false,
    });

    // Orchestrate: wait for the nodes, link them, then periodically link new
    // output streams into the virtual sink.
    let our_pid = std::process::id();
    std::thread::spawn(move || orchestrate(sender, app_name, our_pid));
}

// ---------------------------------------------------------------------------
// PipeWire thread
// ---------------------------------------------------------------------------

fn pw_thread(receiver: pipewire::channel::Receiver<Cmd>) {
    let mainloop = MainLoopRc::new(None).expect("failed to create PipeWire mainloop");
    let context = ContextRc::new(&mainloop, None).expect("failed to create PipeWire context");
    let core = context
        .connect_rc(None)
        .expect("failed to connect to PipeWire core");
    let registry = core
        .get_registry_rc()
        .expect("failed to get PipeWire registry");

    // Keep created proxies alive while the thread runs.
    let proxies: Rc<RefCell<Vec<ProxyWrapper>>> = Rc::new(RefCell::new(Vec::new()));

    let _receiver = receiver.attach(&mainloop.loop_(), {
        let core = core.clone();
        let proxies = proxies.clone();
        move |cmd| match cmd {
            Cmd::CreateSink {
                name,
                position,
                channels,
                permanent,
            } => {
                let sink = create_sink(name, position, channels, permanent, &core);
                proxies.borrow_mut().push(ProxyWrapper::new(sink.upcast()));
            }
            Cmd::CreateSource {
                name,
                position,
                channels,
                permanent,
            } => {
                let source = create_source(name, position, channels, permanent, &core);
                proxies.borrow_mut().push(ProxyWrapper::new(source.upcast()));
            }
            Cmd::LinkNodesNameToId {
                output_nodes_name,
                input_node_id,
                permanent,
            } => {
                for link in link_nodes_name_to_id(output_nodes_name, input_node_id, permanent, &core)
                {
                    proxies.borrow_mut().push(ProxyWrapper::new(link.upcast()));
                }
            }
        }
    });

    let _listener = registry
        .add_listener_local()
        .global({
            let all_data = ALL_DATA.clone();
            move |object| save_object(&object, &all_data)
        })
        .global_remove({
            let all_data = ALL_DATA.clone();
            let proxies = proxies.clone();
            move |id| {
                all_data.lock().unwrap().remove(&id);
                proxies.borrow_mut().retain(|p| p.get_global_id() != id);
            }
        })
        .register();

    mainloop.run();
}

// ---------------------------------------------------------------------------
// Registry -> ALL_DATA
// ---------------------------------------------------------------------------

fn save_object(object: &GlobalObject<&DictRef>, all_data: &Arc<Mutex<HashMap<u32, Data>>>) {
    match object.type_ {
        ObjectType::Node => save_node(object, all_data),
        ObjectType::Port => save_port(object, all_data),
        ObjectType::Client => save_client(object, all_data),
        _ => {}
    }
}

fn save_node(node: &GlobalObject<&DictRef>, all_data: &Arc<Mutex<HashMap<u32, Data>>>) {
    let Some(props) = node.props else { return };
    let id = node.id;

    let name = props
        .get("node.nick")
        .or_else(|| props.get("node.name"))
        .unwrap_or_default()
        .to_string();

    let node_props = props
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    all_data.lock().unwrap().insert(
        id,
        Data::Node(NodeInfo {
            id,
            name,
            props: node_props,
            ports: Vec::new(),
        }),
    );
}

fn save_port(port: &GlobalObject<&DictRef>, all_data: &Arc<Mutex<HashMap<u32, Data>>>) {
    let Some(props) = port.props else { return };
    let id = port.id;

    let node_id: u32 = props
        .get("node.id")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let name = props.get("port.name").unwrap_or_default().to_string();
    let direction = props
        .get("port.direction")
        .map(|d| {
            if d.contains("in") {
                "Input"
            } else if d.contains("out") {
                "Output"
            } else {
                ""
            }
        })
        .unwrap_or_default()
        .to_string();

    let pw_port = Port {
        id,
        node_id,
        name,
        direction,
    };

    let mut data = all_data.lock().unwrap();
    data.insert(id, Data::Port(pw_port.clone()));
    for value in data.values_mut() {
        if let Data::Node(node) = value {
            if node.id == node_id {
                node.ports.push(pw_port.clone());
            }
        }
    }
}

fn save_client(client: &GlobalObject<&DictRef>, all_data: &Arc<Mutex<HashMap<u32, Data>>>) {
    let Some(props) = client.props else { return };
    let id = client.id;

    let pid: u32 = props
        .get("pipewire.sec.pid")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let application_name = props.get("application.name").unwrap_or_default().to_string();

    all_data.lock().unwrap().insert(
        id,
        Data::Client(ClientInfo {
            id,
            pid,
            application_name,
        }),
    );
}

// ---------------------------------------------------------------------------
// Node / link creation
// ---------------------------------------------------------------------------

fn create_sink(
    sink_name: String,
    audio_position: String,
    channel_count: u32,
    permanent: bool,
    core: &Core,
) -> Node {
    core.create_object::<Node>(
        "adapter",
        &properties! {
            "media.class" => "Audio/Sink/Virtual",
            "node.name" => "node-pipewire:".to_string() + &sink_name,
            "node.nick" => sink_name,
            "audio.position" => audio_position,
            "audio.channels" => channel_count.to_string(),
            "factory.name" => "support.null-audio-sink",
            "object.linger" => permanent.to_string(),
        },
    )
    .expect("error creating virtual sink")
}

fn create_source(
    source_name: String,
    audio_position: String,
    channel_count: u32,
    permanent: bool,
    core: &Core,
) -> Node {
    core.create_object::<Node>(
        "adapter",
        &properties! {
            "media.class" => "Audio/Source/Virtual",
            "node.name" => "node-pipewire:".to_string() + &source_name,
            "node.nick" => source_name,
            "audio.position" => audio_position,
            "audio.channels" => channel_count.to_string(),
            "factory.name" => "support.null-audio-sink",
            "object.linger" => permanent.to_string(),
        },
    )
    .expect("error creating virtual source")
}

fn link_ports(
    input_port_id: u32,
    output_port_id: u32,
    permanent: bool,
    core: &Core,
) -> Link {
    let all_data = ALL_DATA.lock().unwrap();
    let ports: Vec<Port> = all_data
        .values()
        .filter_map(|d| match d {
            Data::Port(p) => Some(p.clone()),
            _ => None,
        })
        .collect();
    drop(all_data);

    let input_port = ports
        .iter()
        .find(|p| p.id == input_port_id)
        .expect("missing input port");
    let output_port = ports
        .iter()
        .find(|p| p.id == output_port_id)
        .expect("missing output port");

    core.create_object::<Link>(
        "link-factory",
        &properties! {
            "link.output.port" => output_port.id.to_string(),
            "link.input.port" => input_port.id.to_string(),
            "link.output.node" => output_port.node_id.to_string(),
            "link.input.node" => input_port.node_id.to_string(),
            "object.linger" => permanent.to_string(),
        },
    )
    .expect("error creating link")
}

fn link_nodes_name_to_id(
    nodes_name: String,
    input_node_id: u32,
    permanent: bool,
    core: &Core,
) -> Vec<Link> {
    let all_data = ALL_DATA.lock().unwrap();

    let nodes: Vec<NodeInfo> = all_data
        .values()
        .filter_map(|d| match d {
            Data::Node(n) => Some(n.clone()),
            _ => None,
        })
        .collect();
    drop(all_data);

    let output_nodes: Vec<&NodeInfo> = nodes.iter().filter(|n| n.name == nodes_name).collect();

    let mut output_ports: Vec<&Port> = Vec::new();
    for node in &output_nodes {
        for port in &node.ports {
            if port.direction == "Output" {
                output_ports.push(port);
            }
        }
    }

    let mut output_ports_fr = Vec::new();
    let mut output_ports_fl = Vec::new();
    for port in &output_ports {
        if port.name.contains("FR") {
            output_ports_fr.push(*port);
        } else if port.name.contains("FL") {
            output_ports_fl.push(*port);
        }
    }

    let input_node = nodes
        .iter()
        .find(|n| n.id == input_node_id)
        .expect("missing input node");

    let input_ports: Vec<&Port> = input_node
        .ports
        .iter()
        .filter(|p| p.direction == "Input")
        .collect();

    let input_port_fr = input_ports.iter().find(|p| p.name.contains("FR"));
    let input_port_fl = input_ports.iter().find(|p| p.name.contains("FL"));
    let input_port_mono = input_ports.iter().find(|p| p.name.contains("MONO"));

    let mut links = Vec::new();

    if input_port_fr.is_some() && input_port_fl.is_some() {
        for port in &output_ports_fr {
            links.push(link_ports(input_port_fr.unwrap().id, port.id, permanent, core));
        }
        for port in &output_ports_fl {
            links.push(link_ports(input_port_fl.unwrap().id, port.id, permanent, core));
        }
    } else if input_port_mono.is_some() {
        for port in output_ports_fr.iter().chain(output_ports_fl.iter()) {
            links.push(link_ports(input_port_mono.unwrap().id, port.id, permanent, core));
        }
    } else {
        for port in output_ports_fr.iter().chain(output_ports_fl.iter()) {
            for input_port in &input_ports {
                links.push(link_ports(input_port.id, port.id, permanent, core));
            }
        }
    }

    links
}

// ---------------------------------------------------------------------------
// Orchestration (initial link + periodic linking)
// ---------------------------------------------------------------------------

fn find_node_by_name(name: &str) -> Option<NodeInfo> {
    let data = ALL_DATA.lock().unwrap();
    data.values().find_map(|d| match d {
        Data::Node(n) if n.name == name => Some(n.clone()),
        _ => None,
    })
}

fn our_pids(root: u32) -> Vec<u32> {
    fn collect(pid: u32, out: &mut Vec<u32>) {
        out.push(pid);
        if let Ok(children) = std::fs::read_to_string(format!("/proc/{pid}/task/{pid}/children")) {
            for child in children.split_whitespace() {
                if let Ok(cp) = child.parse::<u32>() {
                    collect(cp, out);
                }
            }
        }
    }
    let mut out = Vec::new();
    collect(root, &mut out);
    out
}

fn orchestrate(sender: pipewire::channel::Sender<Cmd>, app_name: String, our_pid: u32) {
    // Wait for the virtual sink and source to be created.
    let sink = loop {
        if let Some(node) = find_node_by_name(SINK_NAME) {
            break node;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let source = loop {
        if let Some(node) = find_node_by_name(SOURCE_NAME) {
            break node;
        }
        std::thread::sleep(Duration::from_millis(100));
    };

    // Route the sink's monitor into the source.
    let _ = sender.send(Cmd::LinkNodesNameToId {
        output_nodes_name: sink.name.clone(),
        input_node_id: source.id,
        permanent: false,
    });

    let pids = our_pids(our_pid);
    let mut saved: HashMap<u32, ()> = HashMap::new();

    loop {
        std::thread::sleep(Duration::from_secs(1));

        // Clients belonging to our own process tree or PulseAudio clients made
        // on our behalf should be ignored (avoid routing our own audio).
        let all_data = ALL_DATA.lock().unwrap();
        let our_client_ids: Vec<u32> = all_data
            .values()
            .filter_map(|d| match d {
                Data::Client(c) if pids.contains(&c.pid) || c.application_name == app_name => {
                    Some(c.id)
                }
                _ => None,
            })
            .collect();

        let nodes: Vec<NodeInfo> = all_data
            .values()
            .filter_map(|d| match d {
                Data::Node(n)
                    if n.props
                        .get("media.class")
                        .map(|s| s == "Stream/Output/Audio")
                        .unwrap_or(false) =>
                {
                    Some(n.clone())
                }
                _ => None,
            })
            .filter(|n| {
                let client_id = n
                    .props
                    .get("client.id")
                    .and_then(|v| v.parse::<u32>().ok())
                    .unwrap_or(0);
                !our_client_ids.contains(&client_id)
            })
            .collect();
        drop(all_data);

        for node in &nodes {
            if !saved.contains_key(&node.id) {
                let _ = sender.send(Cmd::LinkNodesNameToId {
                    output_nodes_name: node.name.clone(),
                    input_node_id: sink.id,
                    permanent: false,
                });
                saved.insert(node.id, ());
            }
        }

        // Drop entries for streams that are gone.
        saved.retain(|id, _| nodes.iter().any(|n| n.id == *id));
    }
}

// ---------------------------------------------------------------------------
// Proxy lifetime helper (keeps created nodes/links alive, tracks global ids)
// ---------------------------------------------------------------------------

struct ProxyWrapper {
    internal: Rc<RefCell<ProxyInternal>>,
}

struct ProxyInternal {
    proxy: Proxy,
    global_id: u32,
    listener: Option<ProxyListener>,
}

impl ProxyWrapper {
    fn new(proxy: Proxy) -> Self {
        let wrapped = Rc::new(RefCell::new(ProxyInternal {
            proxy,
            global_id: 0,
            listener: None,
        }));

        let listener = Some(
            wrapped
                .borrow()
                .proxy
                .add_listener_local()
                .bound({
                    let wrapped = wrapped.clone();
                    move |id| {
                        let mut borrowed = wrapped.borrow_mut();
                        borrowed.global_id = id;
                        borrowed.listener = None;
                    }
                })
                .register(),
        );

        wrapped.borrow_mut().listener = listener;

        Self { internal: wrapped }
    }

    fn get_global_id(&self) -> u32 {
        self.internal.borrow().global_id
    }
}
