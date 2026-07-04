#![cfg(feature = "web")]

use rust_embed::Embed;

#[derive(Embed)]
#[folder = "web/"]
pub struct WebAssets;
