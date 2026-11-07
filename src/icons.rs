//! Loads real application icons on a background thread so the process list
//! never blocks on AppKit image work.

use gpui::{Image, ImageFormat};
use objc2::{
    Encode, Encoding, RefEncode, class, msg_send,
    rc::{Allocated, Retained, autoreleasepool},
    runtime::AnyObject,
};
use objc2_foundation::{NSRect, NSString};
use std::{
    collections::HashMap,
    ffi::c_void,
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
    thread,
};

#[repr(transparent)]
#[derive(Clone, Copy)]
struct CgImage(*const c_void);
unsafe impl Encode for CgImage {
    const ENCODING: Encoding = Encoding::Pointer(&Encoding::Struct("CGImage", &[]));
}
unsafe impl RefEncode for CgImage {
    const ENCODING_REF: Encoding = Encoding::Pointer(&<Self as Encode>::ENCODING);
}

const NS_BITMAP_PNG: usize = 4;
const ICON_POINTS: f64 = 64.;

fn png_for(path: &str) -> Option<Vec<u8>> {
    autoreleasepool(|_| unsafe {
        let workspace: Retained<AnyObject> = msg_send![class!(NSWorkspace), sharedWorkspace];
        let path = NSString::from_str(path);
        let icon: Option<Retained<AnyObject>> = msg_send![&*workspace, iconForFile: &*path];
        let icon = icon?;
        let mut rect = NSRect::new(
            objc2_foundation::NSPoint::new(0., 0.),
            objc2_foundation::NSSize::new(ICON_POINTS, ICON_POINTS),
        );
        let none: Option<&AnyObject> = None;
        let image: CgImage = msg_send![
            &*icon,
            CGImageForProposedRect: &raw mut rect,
            context: none,
            hints: none
        ];
        if image.0.is_null() {
            return None;
        }
        let allocated: Allocated<AnyObject> = msg_send![class!(NSBitmapImageRep), alloc];
        let bitmap: Option<Retained<AnyObject>> = msg_send![allocated, initWithCGImage: image];
        let bitmap = bitmap?;
        let properties: Retained<AnyObject> = msg_send![class!(NSDictionary), dictionary];
        let data: Option<Retained<AnyObject>> = msg_send![
            &*bitmap,
            representationUsingType: NS_BITMAP_PNG,
            properties: &*properties
        ];
        let data = data?;
        let bytes: *const c_void = msg_send![&*data, bytes];
        let length: usize = msg_send![&*data, length];
        (!bytes.is_null() && length > 0)
            .then(|| std::slice::from_raw_parts(bytes.cast::<u8>(), length).to_vec())
    })
}

pub struct Icons {
    requests: Sender<String>,
    results: Receiver<(String, Option<Vec<u8>>)>,
    cache: HashMap<String, Option<Arc<Image>>>,
}

impl Icons {
    pub fn start() -> Self {
        let (requests, incoming) = mpsc::channel::<String>();
        let (outgoing, results) = mpsc::channel();
        thread::Builder::new()
            .name("sysly-icons".into())
            .spawn(move || {
                for path in incoming {
                    let png = png_for(&path);
                    if outgoing.send((path, png)).is_err() {
                        break;
                    }
                }
            })
            .expect("start icon worker");
        Self {
            requests,
            results,
            cache: HashMap::new(),
        }
    }

    /// Returns the icon when it is ready and queues it when it is not.
    pub fn get(&mut self, path: &str) -> Option<Arc<Image>> {
        if let Some(icon) = self.cache.get(path) {
            return icon.clone();
        }
        self.cache.insert(path.to_string(), None);
        let _ = self.requests.send(path.to_string());
        None
    }

    pub fn peek(&self, path: &str) -> Option<Arc<Image>> {
        self.cache.get(path).cloned().flatten()
    }

    /// Moves finished icons into the cache. Returns true when any arrived.
    pub fn drain(&mut self) -> bool {
        let mut arrived = false;
        while let Ok((path, png)) = self.results.try_recv() {
            let image = png.map(|bytes| Arc::new(Image::from_bytes(ImageFormat::Png, bytes)));
            arrived |= image.is_some();
            self.cache.insert(path, image);
        }
        arrived
    }
}

/// macOS's Reduce Motion accessibility setting.
pub fn reduce_motion() -> bool {
    unsafe {
        let workspace: Retained<AnyObject> = msg_send![class!(NSWorkspace), sharedWorkspace];
        msg_send![&*workspace, accessibilityDisplayShouldReduceMotion]
    }
}
