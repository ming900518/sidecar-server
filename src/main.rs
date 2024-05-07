use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    thread,
};

use block2::StackBlock;
use objc2::{
    class, msg_send,
    runtime::{AnyClass, AnyObject, MessageReceiver, NSObject},
    sel,
};
use objc2_foundation::{NSArray, NSString};

fn main() {
    let listener = TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], 13700))).unwrap();
    while let Ok((stream, _addr)) = listener.accept() {
        thread::spawn(move || {
            handle_incoming(stream);
        });
    }
}

fn handle_incoming(mut stream: TcpStream) {
    let mut sidecar_core = SidecarCore::new();
    loop {
        let buf_reader = BufReader::new(&mut stream);
        let http_request = buf_reader
            .lines()
            .map(|result| result.unwrap())
            .take_while(|line| !line.is_empty())
            .collect::<Vec<String>>();
        let contents = match http_request.first() {
            Some(method) if method.contains("/devices") => {
                format!(
                    "{:#?}",
                    sidecar_core
                        .device_list
                        .iter()
                        .enumerate()
                        .map(|(i, device)| unsafe {
                            let device_name: *mut NSString = device.send_message(sel!(name), ());
                            let device_name = device_name.as_ref().unwrap().to_string();
                            (i, device_name)
                        })
                        .collect::<BTreeMap<_, _>>()
                )
            }
            Some(method) if method.contains("/connect") => {
                let index = {
                    let Some((_, a)) = method.split_once("/connect/") else {
                        break;
                    };
                    let Some((b, _)) = a.split_once(" HTTP/1.1") else {
                        break;
                    };
                    let Ok(index) = b.parse() else {
                        break;
                    };
                    index
                };
                sidecar_core.connect(index);
                String::from("OK")
            }
            Some(method) if method.contains("/refresh") => {
                sidecar_core.refresh();
                String::from("OK")
            }
            _ => break,
        };
        let status_line = "HTTP/1.1 200 OK";
        let length = contents.len();
        let response = format!("{status_line}\r\nContent-Length: {length}\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{contents}");
        stream.write_all(response.as_bytes()).unwrap();
    }
}

struct SidecarCore {
    manager: *mut NSObject,
    device_list: Vec<&'static AnyObject>,
}

impl SidecarCore {
    fn new() -> Self {
        let _sidecar_core_lib = unsafe {
            libloading::Library::new(
                "/System/Library/PrivateFrameworks/SidecarCore.framework/SidecarCore",
            )
            .unwrap()
        };
        let sidecar_display_manager: &AnyClass = class!(SidecarDisplayManager);
        let manager: *mut NSObject =
            unsafe { sidecar_display_manager.send_message(sel!(sharedManager), ()) };

        let mut new_instance = Self {
            manager,
            device_list: Vec::new(),
        };
        new_instance.refresh();
        new_instance
    }

    fn connect(&mut self, index: usize) {
        let completion_closure = StackBlock::new(|_: &AnyObject| ());

        unsafe {
            let _: bool = msg_send![self.manager.as_ref().unwrap(), connectToDevice: self.device_list[index] completion: &completion_closure];
        }
    }

    fn refresh(&mut self) {
        let devices: *mut NSArray = unsafe { self.manager.send_message(sel!(devices), ()) };
        let device_list = unsafe {
            devices
                .as_ref()
                .unwrap()
                .iter()
                .collect::<Vec<&AnyObject>>()
        };

        self.device_list = device_list;
    }
}
