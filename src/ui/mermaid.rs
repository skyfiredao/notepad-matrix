use std::cell::RefCell;

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
        let script = format!(
            "/*{}*/\n{}\nwindow.__mermaidSources={input};window.__mermaidDark={dark};\n{}",
            include_str!("../../assets/mermaid/LICENSE"),
            include_str!("../../assets/mermaid/mermaid-12.3.0.min.js"),
            include_str!("../../assets/mermaid/render.js"),
        );
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
