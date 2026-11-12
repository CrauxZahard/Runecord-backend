@echo off
setlocal
cd /d "%~dp0"
if not exist data mkdir data
cargo run --release -p runecord-server -- --data data
