FROM aidfuzzer:latest AS fuzzer-base
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
FROM aidfuzzer:latest AS aidfuzzer-firmrebugger
ENV FUZZER=/home/user/xxfuzzer
WORKDIR $FUZZER
COPY --from=tcc-build /tinycc $FUZZER/tinycc
COPY --chown=user ./frb_patches/ $FUZZER/frb_patches/
RUN git apply $FUZZER/frb_patches/firmrebugger_qemu.patch
RUN mv frb_patches/firmrebugger.c qemu-7.2.0/accel/tcg/firmrebugger.c && \
    mv frb_patches/firmrebugger.h qemu-7.2.0/include/tcg/firmrebugger.h

# Configure and build the modified QEMU 7.2.0 (ARM softmmu only, produces libqemu-system-arm.so)
RUN cd qemu-7.2.0 && \
    mkdir -p build && \
    cd build && \
    ../configure \
        --target-list=arm-softmmu \
        --disable-docs \
        --disable-gtk \
        --disable-sdl \
        --disable-opengl \
        --disable-werror \
    && make -j$(nproc)

# Build the fuzzer framework binaries (iofuzz, simulator, simulator_dbg)
RUN cd framework/scipt && bash compile.sh

USER user

WORKDIR /home/user/xxfuzzer/framework/bin
