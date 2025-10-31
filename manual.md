# wok

`wok` is a an async schedule framework for rust, focused on modularity and extensibility.
`wok` separated data from logic through `Resources`s and `System`s.

```rust
#[derive(Resource)]
#[resource(mutable = true)]
struct Counter(pub u32);

async fn do_something(counter: ResMut<'_, Counter>) {
    counter.0 += 1; 
}
```

Take a look at other posible `Resource` param accessors like:

- `Res<'_, T>`: readonly access
- `ResMut<'_, T>`: mutable access
- `Option<Res<'_, T>>`/`Option<ResMut<'_, T>>`: optional access
- `ResTake<T>`: take ownership

Composed params can also be made through the `Param` derive macro.

```rust
#[derive(Param)]
struct Repository<'p> {
    db: Res<'p, Db>,
    http: Res<'p, HttpClient>,
}
```

## axum

`wok` does have an integration with axum through the `wok_axum` crate. You can find an example [here](examplesbin/axum_person_crud/).

## db

`wok` does have a database integration through the `wok_db` crate, that brings nice abstractions over dbs without losing control. 
You can find an example [here](examplesbin/axum_person_crud/).

The only current db supported is `surrealdb` through the `surrealdb` feature flag.

## validation

`wok_axum` has a close relation with the `valigate` crate found at [here](crates/valigate/). The same axum example does use validation through `valigate` and you can find an example [here](examplesbin/axum_person_crud/).

## assets

`wok` supports the easy load of assets through `serde` and the `wok_assets` crate. You can find an example [here](examplesbin/axum_person_crud/).
`wok_assets` supports loading data from `toml` files and ENVIROMENT variables (even loading from `.env`).

>Note that, when loading from ENV, nested data should be embedded as a `toml` value, look at the example [here](examplesbin/axum_person_crud/sample.env).
