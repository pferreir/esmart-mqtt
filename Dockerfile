FROM ubuntu:latest

ARG ARCH=x86_64-unknown-linux-gnu
ARG VERSION

WORKDIR /app

RUN apt-get update && apt-get install -y wget

RUN cd /app && \
    wget https://github.com/pferreir/esmart-mqtt/releases/download/${VERSION}/esmart_mqtt-${ARCH}.tar.gz && \
    tar zxvf esmart_mqtt-${ARCH}.tar.gz && \
    chmod +x esmart_mqtt && \
    rm *.tar.gz

CMD ["/app/esmart_mqtt"]

