use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::Emitter;
use crate::transfer::PeerInfo;

const SERVICE_TYPE: &str = "_tracer._tcp.local.";

pub fn start_discovery(
    peers: Arc<Mutex<HashMap<String, PeerInfo>>>,
    app: tauri::AppHandle,
    device_name: &str,
    port: u16,
) -> Result<(), String> {
    let daemon = ServiceDaemon::new().map_err(|e| e.to_string())?;

    // Advertise own instance
    let host = format!("{}.local.", device_name);
    let properties = [("version", env!("CARGO_PKG_VERSION"))];
    let my_service = ServiceInfo::new(
        SERVICE_TYPE,
        device_name,
        &host,
        "",
        port,
        &properties[..],
    )
    .map_err(|e| e.to_string())?;
    daemon.register(my_service).map_err(|e| e.to_string())?;

    // Browse for peers
    let receiver = daemon.browse(SERVICE_TYPE).map_err(|e| e.to_string())?;
    let own_name = device_name.to_string();

    std::thread::spawn(move || {
        // Keep daemon alive in this thread
        let _daemon = daemon;
        loop {
            match receiver.recv() {
                Ok(ServiceEvent::ServiceResolved(info)) => {
                    let id = info.get_fullname().to_string();
                    // Skip self (exact match to avoid "tracer-1" matching "tracer-10")
                    let own_fullname = format!("{}.{}", own_name, SERVICE_TYPE);
                    if info.get_fullname() == own_fullname {
                        continue;
                    }
                    if let Some(addr) = info.get_addresses().iter().next() {
                        let peer = PeerInfo {
                            id: id.clone(),
                            name: info
                                .get_hostname()
                                .trim_end_matches(".local.")
                                .to_string(),
                            addr: addr.to_string(),
                            port: info.get_port(),
                        };
                        {
                            let mut peers = peers.lock().unwrap();
                            peers.insert(id, peer.clone());
                        }
                        app.emit("peer-discovered", peer).ok();
                    }
                }
                Ok(ServiceEvent::ServiceRemoved(_, fullname)) => {
                    {
                        let mut peers = peers.lock().unwrap();
                        peers.remove(&fullname);
                    }
                    app.emit("peer-lost", &fullname).ok();
                }
                Ok(_) => {}
                Err(e) => {
                    eprintln!("[discovery] mDNS receiver error, stopping discovery: {:?}", e);
                    break;
                }
            }
        }
    });

    Ok(())
}
