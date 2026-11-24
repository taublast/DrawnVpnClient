// Release builds are a plain Windows app with no console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Windows: the same program, started as administrator on Connect, runs the tunnel.
    #[cfg(windows)]
    {
        let args: Vec<String> = std::env::args().collect();
        if args.get(1).is_some_and(|a| a == "--tunnel") {
            std::process::exit(drawnnetc::tunnel_helper(&args));
        }
    }
    drawnnetc::run();
}
