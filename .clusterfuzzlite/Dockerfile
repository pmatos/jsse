FROM gcr.io/oss-fuzz-base/base-builder-rust:v1

# The base image pins an old nightly; jsse needs edition 2024 and let_chains.
RUN rustup toolchain install nightly --profile minimal --component rust-src --no-self-update \
    && rustup default nightly
ENV RUSTUP_TOOLCHAIN=nightly

COPY . $SRC/jsse
COPY .clusterfuzzlite/build.sh $SRC/
WORKDIR $SRC/jsse
