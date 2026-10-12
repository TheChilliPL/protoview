use tracing::debug;
use tracing_subscriber::filter::{Targets, LevelFilter};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use crate::app::ProtoViewApp;

mod app;

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    let native_opts = eframe::NativeOptions::default();
    eframe::run_native(
        "ProtoView",
        native_opts,
        Box::new(|cc| Ok(Box::new(ProtoViewApp::new(cc))))
    ).expect("couldn't create native app");
}

#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast;

    console_error_panic_hook::set_once();
    tracing_log::LogTracer::init().unwrap();

    let _ = tracing_subscriber::registry()
        .with(
            Targets::new()
                .with_target("protoview", LevelFilter::DEBUG)
                .with_default(LevelFilter::INFO)
        )
        .with(tracing_wasm::WASMLayer::default())
        .try_init();

    let web_opts = eframe::WebOptions::default();

    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window()
            .expect("no window")
            .document()
            .expect("no document");

        let canvas = document.get_element_by_id("canvas")
            .expect("no canvas element")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("#canvas element not a canvas");

        eframe::WebRunner::new()
            .start(
                canvas,
                web_opts,
                Box::new(|cc| Ok(Box::new(ProtoViewApp::new(cc))))
            ).await.expect("couldn't start app");
    })
}
