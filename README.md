# 🫕 Wok

**Wok** is your go-to crate for building **rusty, composable apps**.  
Inspired by Bevy’s plugin + system model, Wok makes it easy to integrate async runtimes, configs, and web frameworks like Axum — all with a unified plugin system.

## Quick Start

Create an app and add systems with different schedules:

```rust
use wok::prelude::*;

App::default()
    .add_system(Startup, load_config)
    .run(
        RuntimeCfg::default().with_async::<TokioRt>(),
        wok::setup::runtime
    ).await;

async fn load_config() {
    println!("Config loaded!");
}
```

## Plugins

Compose your app with `Plugin`s!

```rust
use wok::prelude::*;

struct MyConfigPlugin;
impl Plugin for MyConfigPlugin {
    fn setup(self, app: &mut App) {
        app.add_system(Startup, load_config);
    }
}

App::default()
    .add_plugin(MyConfigPlugin);

async fn load_config() {}
```

## Configuration

Easily load your config!
```rust
use wok::prelude::*;
use wok_assets::*;

#[derive(serde::Deserialize, Resource, Debug)]
struct MyConfig {
    // ...
}

App::default()
    .add_system(Startup, load_config)
    .add_system(Startup, log_config); // Automatically waits for `load_config` to finish!

async fn load_config(config: AssetInit<'_, MyConfig>) {
    config.with(TomlLoader("config.toml"));
}

async fn log_config(config: Res<'_, MyConfig>) {
    println!("Config: {config:#?}"); 
}
```

## 🌐 Axum Integration

Integrate with other crates like axum!

```rust
use wok::prelude::*;
use wok::setup::TokioRt;
use wok_axum::*;

let result = App::default()
    .add_plugin(AxumPlugin)
    .add_system(Startup, load)
    .add_system(Route("/"), get(hello_world))
    .run(
        RuntimeCfg::default().with_async::<TokioRt>(),
        wok_axum::serve
    ).await;

async fn load(mut commands: Commands<'_>) -> Result<(), WokUnkownError> {
    let addr = SocketAddrs::new("127.0.0.1:3000").await?;
    commands.insert_resource(addr);

    Ok(())
}

async hello_world() -> String {
    "Hello World!".to_string()
}
```

## Warning

This crate is still in development, changes will probably be breaking, so use it at your own risk!
