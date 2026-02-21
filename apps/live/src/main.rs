fn main() {
	runtime::init();

	let mode_key = std::env::var("STRUXIS_MODE")
		.unwrap_or_else(|_| "ctp".to_string())
		.to_ascii_lowercase();
	let mode = match mode_key.as_str() {
		"binance" => runtime::RuntimeMode::Binance,
		_ => runtime::RuntimeMode::Ctp,
	};

	runtime::run_live_with_mode(mode);
	println!("struxis live runtime done");
}
