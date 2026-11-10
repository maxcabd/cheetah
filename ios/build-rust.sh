#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios

cargo build -p runtime --target aarch64-apple-ios --release --features source-map
cargo build -p runtime --target aarch64-apple-ios-sim --release --features source-map
cargo build -p runtime --target x86_64-apple-ios --release --features source-map

mkdir -p target/sim-universal
lipo -create \
  target/aarch64-apple-ios-sim/release/libruntime.a \
  target/x86_64-apple-ios/release/libruntime.a \
  -output target/sim-universal/libruntime.a

rm -rf ios/Libs/Runtime.xcframework
mkdir -p ios/Libs

xcodebuild -create-xcframework \
  -library target/aarch64-apple-ios/release/libruntime.a \
  -library target/sim-universal/libruntime.a \
  -output ios/Libs/Runtime.xcframework

echo "Wrote ios/Libs/Runtime.xcframework"
