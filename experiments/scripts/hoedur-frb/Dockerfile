FROM hoedur-fuzzware AS fuzzer-base
ENV PIP_NO_CACHE_DIR=1

# ---- Stage 1: Building TinyCC ----
FROM fuzzer-base AS tcc-build
USER root
RUN git clone https://github.com/TinyCC/tinycc.git /tinycc && \
    cd /tinycc && \
    git checkout 7c23c48a936d2bf2ea181af58d698d1816a52e6f
COPY --chown=user ./frb_patches/frb_tcc.patch /tinycc/frb_tcc.patch
RUN cd /tinycc && git apply /tinycc/frb_tcc.patch
RUN cd /tinycc && ./configure && make


# ---- Stage 2: Extend fuzzer FirmReBugger ----
FROM hoedur-fuzzware AS hoedur-firmrebugger
ENV FUZZER=/home/user/hoedur
ENV PATH="${PATH}:/home/user/.cargo/bin"
ENV LD_LIBRARY_PATH="/home/user/.cargo/bin/"
WORKDIR $FUZZER
RUN git apply -p1 patches/hoedur.patch
COPY --from=tcc-build /tinycc $FUZZER/tinycc
COPY --chown=user ./frb_patches/ $FUZZER/frb_patches/
RUN git apply $FUZZER/frb_patches/firmrebugger_hoedur_rust.patch
RUN cargo install --no-track --force --path $FUZZER/hoedur \
    --bin hoedur-arm  && \
    cp $FUZZER/target/release/libqemu-system-arm.release.so $HOME/.cargo/bin/ && \
    cp $FUZZER/target/release/hoedur-arm $HOME/.cargo/bin/hoedur-arm-frb && \
    cargo clean

# Same for Steamfuzz
RUN git reset --hard HEAD
RUN cargo install --no-track --force --path $FUZZER/hoedur \
    --bin hoedur-arm  && \
    cp $FUZZER/target/release/hoedur-arm $HOME/.cargo/bin/steamfuzz-arm-frb && \
    cargo clean

# # ---- Install FirmReBugger ----
# USER user
# RUN pip3 install --user uv
# ENV PATH=$PATH:/home/user/.local/bin

# COPY --chown=user pyproject.toml uv.lock .python-version README.md /home/user/firmrebugger/
# RUN mkdir -p /home/user/firmrebugger/src/firmrebugger /home/user/firmrebugger/target
# WORKDIR /home/user/firmrebugger/target

# # ---- Final Setup ----
# ENV FIRMREBUGGER_BASE_DIR=/home/user/firmrebugger
# ENV HOEDUR_BASE_DIR=$FUZZER/hoedur
# ENTRYPOINT ["uv", "run", "frb", "bug-analyzer", "."]
