[![Main](https://github.com/28Smiles/share.rs/actions/workflows/test.yml/badge.svg)](https://github.com/28Smiles/share.rs/actions/workflows/test.yml)
[![MIT licensed](https://img.shields.io/badge/license-MIT-blue.svg)](./LICENSE)
[![Latest Stable](https://img.shields.io/github/v/release/28Smiles/share.rs?label=latest%20stable)](https://github.com/28Smiles/share.rs/releases/latest)
[![Latest Release](https://img.shields.io/github/v/release/28Smiles/share.rs?include_prereleases&label=latest%20release)](https://github.com/28Smiles/share.rs/releases)
[![codecov](https://codecov.io/gh/28Smiles/share.rs/branch/master/graph/badge.svg?token=Td24qudkuq)](https://codecov.io/gh/28Smiles/share.rs)

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
