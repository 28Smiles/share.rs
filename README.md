# share.rs
A simple fileshare server for shareX

## Build and Run

```
  git clone https://github.com/28Smiles/share.rs.git shares
  cd shares
  cargo build --release
  cp ./target/release/shares ./shares
```

Configure your keys and users in the `config.yml` and launch with:
```
  ./shares
```

I suggest to use an proxy like nginx to get ssl working.

## ShareX Setup

![](https://github.com/28Smiles/share.rs/blob/master/store/setup_sharex_1.png?raw=true)
![](https://github.com/28Smiles/share.rs/blob/master/store/setup_sharex_2.png?raw=true)
