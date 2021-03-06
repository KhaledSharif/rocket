FROM rust:1.33

RUN rustup toolchain install nightly
RUN rustup default nightly

WORKDIR /usr/src/myapp
COPY ./Cargo.toml .
COPY ./src ./src
RUN cargo build --release

CMD ROCKET_ADDRESS=0.0.0.0 cargo run --release
