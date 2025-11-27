ARG BUILD_FROM
FROM $BUILD_FROM

# Install Python dependencies
RUN apk add --no-cache python3 py3-pip iproute2
RUN pip3 install --break-system-packages python-can

COPY run.sh /
COPY gateway.py /

RUN chmod a+x /run.sh

WORKDIR /usr/src/app

CMD [ "/run.sh" ]
