FROM ubuntu:18.04

RUN apt update && apt install -y clang curl git libfdt-dev libglib2.0-dev \
        libpixman-1-dev libxcb-shape0-dev libxcb-xfixes0-dev ninja-build \
        patchelf pkg-config python3-psutil zstd curl wget
RUN useradd -u 1001 -m user
RUN mkdir copied && mkdir shared

COPY ./setup/ /home/user/copied/
RUN chown -R user /home/user/

USER user

WORKDIR /home/user/copied
RUN chmod +x ./rustup-init.sh && \
	./rustup-init.sh --default-toolchain stable -y 
	# && \
	#. "$HOME/.cargo/env" && \
	# cargo install --path hoedur/ --bin hoedur-arm
# USER root
# RUN cp target/release/libqemu-system-arm.release.so /usr/lib/
# USER user

WORKDIR /home/user/
ENTRYPOINT cp -r shared/ shared_copied && cp shared/run_fuzzer.sh . && /bin/bash
