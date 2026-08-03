pub mod png;

use crossbeam_channel::{Sender, Receiver, unbounded};
use rfd::AsyncFileDialog;

pub enum FileAction {
    OpenedImage(Vec<u8>),
    Exported,
}

pub struct IoHandler {
    pub sender: Sender<FileAction>,
    pub receiver: Receiver<FileAction>,
}

impl IoHandler {
    pub fn new() -> Self {
        let (sender, receiver) = unbounded();
        Self { sender, receiver }
    }
}

pub fn trigger_open_file(sender: Sender<FileAction>) {
    let task = async move {
        if let Some(file) = AsyncFileDialog::new()
            .add_filter("Image", &["png"])
            .pick_file()
            .await 
        {
            let data = file.read().await;
            let _ = sender.send(FileAction::OpenedImage(data));
        }
    };
    
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_futures::spawn_local(task);
    
    #[cfg(not(target_arch = "wasm32"))]
    std::thread::spawn(move || {
        pollster::block_on(task);
    });
}

pub fn trigger_export_png(data: Vec<u8>, sender: Sender<FileAction>) {
    let task = async move {
        if let Some(file) = AsyncFileDialog::new()
            .add_filter("PNG Image", &["png"])
            .set_file_name("export.png")
            .save_file()
            .await 
        {
            // rfd handles writing for us cross-platform
            if file.write(&data).await.is_ok() {
                let _ = sender.send(FileAction::Exported);
            }
        }
    };
    
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_futures::spawn_local(task);
    
    #[cfg(not(target_arch = "wasm32"))]
    std::thread::spawn(move || {
        pollster::block_on(task);
    });
}
