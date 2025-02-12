FROM hypriot/rpi-alpine-scratch
ENV PYTHONUNBUFFERED=1
RUN apk add --update --no-cache python3 && ln -sf python3 /usr/bin/python
RUN apk add make build-base python3-dev portaudio-dev python3-pyaudio python3-scipy
RUN python3 -m ensurepip
RUN pip3 install --no-cache --upgrade pip setuptools
WORKDIR /home
COPY requirements.txt requirements.txt
RUN python3 -m pip install -r requirements.txt
COPY makefile makefile
COPY . .
CMD ["make", "startup"]