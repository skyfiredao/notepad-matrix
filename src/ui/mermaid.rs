use std::cell::RefCell;
use std::io::Read;

use futures::channel::oneshot;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{DefinedClass, MainThreadOnly, define_class, msg_send};
use objc2_foundation::{
    MainThreadMarker, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
};
use objc2_web_kit::{
    WKScriptMessage, WKScriptMessageHandler, WKUserContentController, WKUserScript,
    WKUserScriptInjectionTime, WKWebView, WKWebViewConfiguration, WKWebsiteDataStore,
};

type Response = Result<String, String>;

struct MessageState {
    sender: RefCell<Option<oneshot::Sender<Response>>>,
}

define_class!(
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = MessageState]
    struct MermaidMessageHandler;

    unsafe impl NSObjectProtocol for MermaidMessageHandler {}

    unsafe impl WKScriptMessageHandler for MermaidMessageHandler {
        #[unsafe(method(userContentController:didReceiveScriptMessage:))]
        unsafe fn receive(&self, _: &WKUserContentController, message: &WKScriptMessage) {
            if !unsafe { message.frameInfo().isMainFrame() } {
                return;
            }
            let result = unsafe { message.body() }
                .downcast::<NSString>()
                .map(|body| body.to_string())
                .map_err(|_| "Mermaid returned an invalid response".to_string());
            if let Some(sender) = self.ivars().sender.borrow_mut().take() {
                let _ = sender.send(result);
            }
        }
    }
);

pub(super) struct Renderer {
    view: Retained<WKWebView>,
    controller: Retained<WKUserContentController>,
}

impl Renderer {
    pub(super) fn start(
        sources: &[&str],
        dark: bool,
    ) -> Result<(Self, oneshot::Receiver<Response>), String> {
        let mtm = MainThreadMarker::new().ok_or("Mermaid rendering requires the main thread")?;
        let input = serde_json::to_string(sources).map_err(|error| error.to_string())?;
        let source_length: usize = include!(concat!(env!("OUT_DIR"), "/mermaid-source-len.rs"));
        let render = include_str!("../../assets/mermaid/render.js");
        let mut script = String::with_capacity(source_length.saturating_add(input.len()).saturating_add(render.len()).saturating_add(128));
        flate2::bufread::DeflateDecoder::new(include_bytes!(concat!(env!("OUT_DIR"), "/mermaid-script.deflate")).as_slice())
            .read_to_string(&mut script).map_err(|error| format!("Mermaid resource could not be decoded: {error}"))?;
        if script.len() != source_length {
            return Err("Mermaid resource has an invalid decoded length".into());
        }
        script.push_str("\nwindow.__mermaidSources=");
        script.push_str(&input);
        script.push_str(";window.__mermaidDark=");
        script.push_str(if dark { "true" } else { "false" });
        script.push_str(";\n");
        script.push_str(render);
        let (sender, receiver) = oneshot::channel();
        let allocated = MermaidMessageHandler::alloc(mtm).set_ivars(MessageState {
            sender: RefCell::new(Some(sender)),
        });
        // SAFETY: NSObject initialization and typed WebKit configuration execute on the main thread.
        let renderer = unsafe {
            let handler: Retained<MermaidMessageHandler> = msg_send![super(allocated), init];
            let controller = WKUserContentController::new(mtm);
            controller.addScriptMessageHandler_name(
                ProtocolObject::from_ref(&*handler),
                &NSString::from_str("mermaid"),
            );
            let script = WKUserScript::initWithSource_injectionTime_forMainFrameOnly(
                WKUserScript::alloc(mtm),
                &NSString::from_str(&script),
                WKUserScriptInjectionTime::AtDocumentStart,
                true,
            );
            controller.addUserScript(&script);
            let configuration = WKWebViewConfiguration::new(mtm);
            configuration.setUserContentController(&controller);
            configuration.setWebsiteDataStore(&WKWebsiteDataStore::nonPersistentDataStore(mtm));
            configuration
                .preferences()
                .setJavaScriptCanOpenWindowsAutomatically(false);
            let view = WKWebView::initWithFrame_configuration(
                WKWebView::alloc(mtm),
                NSRect::new(NSPoint::new(0., 0.), NSSize::new(1024., 768.)),
                &configuration,
            );
            view.loadHTMLString_baseURL(&NSString::from_str(
                "<!doctype html><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; script-src 'none'; style-src 'unsafe-inline'; img-src data: blob:; connect-src 'none'; font-src 'none'; frame-src 'none'; object-src 'none'; base-uri 'none'\"><body></body>",
            ), None);
            Self { view, controller }
        };
        Ok((renderer, receiver))
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        // SAFETY: The preview owns this renderer and releases it on the GPUI main thread.
        unsafe {
            self.view.stopLoading();
            self.controller
                .removeScriptMessageHandlerForName(&NSString::from_str("mermaid"));
            self.controller.removeAllUserScripts();
        }
    }
}
