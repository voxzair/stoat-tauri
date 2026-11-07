use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};

use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};

static ENABLED: AtomicBool = AtomicBool::new(false);
static CLIENT: Mutex<Option<DiscordIpcClient>> = Mutex::new(None);

const CLIENT_ID: &str = "872068124005007420";

pub fn init() {
    if ENABLED.swap(true, Ordering::SeqCst) {
        return; // already running
    }
    std::thread::spawn(worker);
}

pub fn destroy() {
    ENABLED.store(false, Ordering::SeqCst);
    if let Ok(mut guard) = CLIENT.lock() {
        if let Some(mut client) = guard.take() {
            let _ = client.close();
        }
    }
}

fn worker() {
    while ENABLED.load(Ordering::SeqCst) {
        match connect_once() {
            Ok(client) => {
                *CLIENT.lock().unwrap() = Some(client);
                // The client stays alive via its internal reader thread. Park
                // here until destroy() flips ENABLED and closes the client.
                while ENABLED.load(Ordering::SeqCst) {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
            }
            Err(_) => std::thread::sleep(std::time::Duration::from_secs(10)),
        }
    }
}

fn connect_once() -> Result<DiscordIpcClient, Box<dyn std::error::Error>> {
    let mut client = DiscordIpcClient::new(CLIENT_ID)?;
    client.connect()?;
    client.set_activity(
        activity::Activity::new()
            .state("stoat.chat")
            .details("Chatting with others")
            .assets(activity::Assets::new().large_image("qr").large_text("Join Stoat!"))
            .buttons(vec![activity::Button::new(
                "Join Stoat",
                "https://stoat.chat/",
            )]),
    )?;
    Ok(client)
}
