# wok
`wok` is an **async scheduling framework for Rust**, focused on **modularity** and **extensibility**.  
It separates data from logic through the use of `Resource`s and `System`s.

```rust
#[derive(Resource)]
#[resource(mutable = true)]
struct Counter(pub u32);

async fn do_something(counter: ResMut<'_, Counter>) {
    counter.0 += 1; 
}
```
### Resource access
Systems can access resources in different ways, depending on their intent:
- `Res<'_, T>` — read-only access
- `ResMut<'_, T>` — mutable access
- `Option<Res<'_, T>>` / `Option<ResMut<'_, T>>` — optional access
- `ResTake<T>` — takes ownership and removes the resource

You can also group parameters into a single type using the `#[derive(Param)]` macro:

```rust
#[derive(Param)]
struct Repository<'p> {
    db: Res<'p, Db>,
    http: Res<'p, HttpClient>,
}
```

## Axum integration
`wok` integrates with [`axum`](https://crates.io/crates/axum) through the `wok_axum` crate.
This enables adding routes and middleware as systems, while keeping all of wok’s scheduling and dependency features.
>Example: [examplesbin/axum_person_crud/](examplesbin/axum_person_crud/)

## Database integration
Database support is provided by the `wok_db` crate.
It offers flexible abstractions over different backends while keeping full control in the user’s hands.

Currently, the only supported database is `SurrealDB`, available under the surrealdb feature flag.
>Example: [examplesbin/axum_person_crud/](examplesbin/axum_person_crud/)

## Validation
Validation in `wok` is powered by the [`valigate`](crates/valigate/) crate — a composable and type-safe validation library.
It integrates naturally with `wok_axum` extractors, allowing request data to be validated automatically before reaching systems.
>Example: [examplesbin/axum_person_crud/](examplesbin/axum_person_crud/)

## Assets and configuration
Configuration and asset loading are handled by the `wok_assets` crate.
It uses serde to load data from both TOML files and environment variables, including .env files.

>Example: [examplesbin/axum_person_crud/](examplesbin/axum_person_crud/)

>Note: When loading from environment variables, nested data should be embedded as TOML values.
>Example: [examplesbin/axum_person_crud/sample.env](examplesbin/axum_person_crud/sample.env)
