use tracing::info;
use tracing_subscriber;

const BANNER: &str = r#"
===============================================
              _          _  _ ___  _ 
             /_`_   _   /_`/_///_//_`
            /_,/_|_\/_/._// /// \._/ 
                    _/               
===============================================
"#;

pub fn init_logger() {
    load_banner();
    tracing_subscriber::fmt::init();

    let version = env!("CARGO_PKG_VERSION");
    let name = env!("CARGO_PKG_NAME");
    let author = env!("CARGO_PKG_AUTHORS");
    info!("c {name}:v{version} by {author}");
}

#[inline]
fn load_banner() {
    println!("{}", BANNER);
}
