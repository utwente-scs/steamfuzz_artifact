FROM fuzzware:fuzzware-hoedur-eval

USER root
RUN apt update -q
RUN apt install -qy \
    clang \
    curl \
    git \
    libfdt-dev \
    libglib2.0-dev \
    libpixman-1-dev \
    libxcb-shape0-dev \
    libxcb-xfixes0-dev \
    ninja-build \
    patchelf \
    pkg-config \
    python3-psutil \
    zstd

# install Rust
USER user
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --no-modify-path --default-toolchain 1.88.0
ENV PATH="${PATH}:/home/user/.cargo/bin"
ENV LD_LIBRARY_PATH="/home/user/.cargo/bin/"

# set git user
ENV GIT_COMMITTER_NAME=Hoedur
ENV GIT_COMMITTER_EMAIL=user@hoedur

# TODO: switch the copy to the git commands
# get hoedur + add eval patches
ENV HOEDUR=/home/user/hoedur

COPY --chown=user steamfuzz $HOEDUR/
COPY --chown=user patches/ $HOEDUR/patches/
WORKDIR $HOEDUR

RUN git init . && \
    git config user.email "build@build.com" && \
    git config user.name "Build" && \
    git add . && \
    git commit -m "init"

# install hoedur (emulator + fuzzer)
# RUN cargo install --path $HOEDUR/hoedur \
#     --bin hoedur-arm && \
#     cp $HOEDUR/target/release/libqemu-system-arm.release.so $HOME/.cargo/bin/ && \
    # install hoedur variants
RUN patches/build.sh steamfuzz && \
    cargo install --path $HOEDUR/hoedur-analyze --bin hoedur-reproducer && \
    mv /home/user/.cargo/bin/hoedur-reproducer /home/user/.cargo/bin/steamfuzz-reproducer && \
    cargo clean && \
    patches/build.sh steamfuzz_ablation1 && \
    cargo clean && \
    patches/build.sh steamfuzz_ablation2 && \
    cargo clean && \
    patches/build.sh && \
    cp $HOEDUR/target/release/libqemu-system-arm.release.so $HOME/.cargo/bin/ && \
    cargo install --path $HOEDUR/hoedur \
    --bin hoedur-convert-fuzzware-config && \
    cargo install --path $HOEDUR/hoedur-analyze \
    --bin hoedur-coverage-list \
    --bin hoedur-eval-crash \
    --bin hoedur-eval-executions \
    --bin hoedur-merge-report \
    --bin hoedur-plot-data \
    --bin hoedur-reproducer && \
    cargo clean && \
    git reset --hard HEAD && \
    git clean -fd


# add hoedur scripts to python path
ENV PYTHONPATH="/home/user/hoedur/scripts/"

# set BASEDIR / WORKDIR
ENV BASEDIR="/home/user/hoedur-experiments"
WORKDIR $BASEDIR