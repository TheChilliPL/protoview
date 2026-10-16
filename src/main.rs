use crate::app::ProtoViewApp;

mod app;
mod protobuf;

#[cfg(not(target_arch = "wasm32"))]
#[derive(clap::Parser)]
struct Cli {
    /// ProtoBuf data file path to open
    input: Option<std::path::PathBuf>,
    /// ProtoBuf schema file path to open
    #[arg(short, long)]
    schema: Option<std::path::PathBuf>,
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    tracing_log::LogTracer::init().unwrap();
    let _ = tracing_subscriber::fmt::try_init();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let _rt = rt.enter();

    let cli = <Cli as clap::Parser>::parse();

    let buffer = cli
        .input
        .map(|path| std::fs::read(path).unwrap().into_boxed_slice());
    let schema_data = cli
        .schema
        .map(|path| std::fs::read(path).unwrap().into_boxed_slice());
    let schema = schema_data.map(|d| protobuf::schema::ProtobufSchema::compile_new(d).unwrap());

    let native_opts = eframe::NativeOptions::default();
    eframe::run_native(
        "ProtoView",
        native_opts,
        Box::new(|cc| Ok(Box::new(ProtoViewApp::new_with_opts(cc, buffer, schema)))),
    )
    .expect("couldn't create native app");
}

#[cfg(target_arch = "wasm32")]
fn try_create_panic_message(
    panic_hook_info: &std::panic::PanicHookInfo,
    document: &web_sys::Document,
) -> eyre::Result<()> {
    let body = document.body().ok_or_eyre("no body in document")?;
    let div = document
        .create_element("div")
        .map_err(|e| eyre!("{:?}", e))?;
    div.set_class_name("error_message");
    div.set_inner_html(&format!(
        "<h1>🦀 Rust panicked!</h1><pre>{}</pre>",
        panic_hook_info,
    ));
    body.append_child(&div).map_err(|e| eyre!("{:?}", e))?;
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn panic_hook(panic_hook_info: &std::panic::PanicHookInfo) {
    if let Some(window) = web_sys::window()
        && let Some(document) = window.document()
    {
        let _ = try_create_panic_message(panic_hook_info, &document);
    }

    console_error_panic_hook::hook(panic_hook_info);
}

#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast;
    use tracing_subscriber::filter::{LevelFilter, Targets};
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    std::panic::set_hook(Box::new(panic_hook));

    tracing_log::LogTracer::init().unwrap();

    let _ = tracing_subscriber::registry()
        .with(
            Targets::new()
                .with_target("protoview", LevelFilter::DEBUG)
                .with_default(LevelFilter::INFO),
        )
        .with(tracing_wasm::WASMLayer::default())
        .try_init();

    let web_opts = eframe::WebOptions::default();

    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window()
            .expect("no window")
            .document()
            .expect("no document");

        let canvas = document
            .get_element_by_id("canvas")
            .expect("no canvas element")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("#canvas element not a canvas");

        eframe::WebRunner::new()
            .start(
                canvas,
                web_opts,
                Box::new(|cc| Ok(Box::new(ProtoViewApp::new(cc)))),
            )
            .await
            .expect("couldn't start app");
    })
}
