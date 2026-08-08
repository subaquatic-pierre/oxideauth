FROM rust:1.79.0-slim-bullseye AS build

# View app name in Cargo.toml
ENV APP_NAME=oxideauth

RUN apt-get update && \
    apt-get install \
    pkg-config libssl-dev -y

WORKDIR /build

COPY . /build/

RUN mv .env.prod .env

ENV SQLX_OFFLINE=true

RUN cargo build --release --bin $APP_NAME 
RUN cp ./target/release/$APP_NAME /bin/$APP_NAME

RUN chmod +x /bin/$APP_NAME 

EXPOSE 6000
CMD ["/bin/oxideauth"]


