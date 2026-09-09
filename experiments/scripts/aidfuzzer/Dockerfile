FROM fuzzware:latest

# ARG USER_ID=1000
# ARG GROUP_ID=1000

ENV DEBIAN_FRONTEND=noninteractive

USER root

# Install build dependencies
RUN apt-get update -q && apt-get install -y \
    build-essential \
    clang \
    pkg-config \
    libglib2.0-dev \
    libpixman-1-dev \
    libfdt-dev \
    zlib1g-dev \
    ninja-build \
    python3 \
    python3-pip \
    flex \
    bison \
    python3-psutil \
    python3-virtualenv \
    virtualenvwrapper \
    wget \
    git \
    && pip3 install meson \
    && wget -q https://raw.githubusercontent.com/muellan/clipp/master/include/clipp.h \
         -O /usr/local/include/clipp.h \
    && rm -rf /var/lib/apt/lists/*

# Create a non-root user with a home directory


# RUN groupadd -g ${GROUP_ID} user && \
#     useradd -u ${USER_ID} -g ${GROUP_ID} -m -d /home/user -s /bin/bash user && \
#     chown -R ${USER_ID}:${GROUP_ID} /home/user

# Set up virtualenvwrapper and create the fuzzware virtualenv (packages installed after COPY)
ENV WORKON_HOME=/home/user/.virtualenvs
ENV VIRTUALENVWRAPPER_PYTHON=/usr/bin/python3
RUN bash -c 'source /usr/share/virtualenvwrapper/virtualenvwrapper.sh && mkvirtualenv fuzzware'

RUN echo 'source /usr/share/virtualenvwrapper/virtualenvwrapper.sh && workon fuzzware' >> /home/user/.bashrc

WORKDIR /home/user/xxfuzzer

# Copy entire project into the container
COPY aidfuzzer/ .
COPY patches .

RUN git apply aidfuzzer.patch

# Install pinned Python dependencies into the fuzzware virtualenv
RUN bash -c 'source /usr/share/virtualenvwrapper/virtualenvwrapper.sh \
    && workon fuzzware \
    && pip install -r requirements.txt'

# Overwrite pyvex's arm_spotter with the project-patched version (adds set_current_irq)
RUN cp arm_spotter_patched.py \
    /home/user/.virtualenvs/fuzzware/lib/python3.10/site-packages/pyvex/lifting/gym/arm_spotter.py

# Build the ihex static library used by the simulator
RUN cd framework/lib/ihex && make

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

# Transfer ownership of the working tree and virtualenvs to the non-root user
RUN chown -R user:user /home/user

USER user

WORKDIR /home/user/xxfuzzer/framework/bin
