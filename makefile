MAKEFLAGS += -j2
dockeraddress=115136208505.dkr.ecr.ap-southeast-2.amazonaws.com/pitraincam


clone:
	git clone https://github.com/mhear22/pitraincamera.git && cd pitraincamera && make install && make run

install: setup-pi
	python3 -m pip install -r requirements.txt

setup-pi:
	sudo apt-get install python3-scipy

run:
	cd src/webapp && python3 manage.py runserver 0.0.0.0:8000

listen:
	python3 src/mic/listener.py

startup: 
	make run& make listen

pic:
	libcamera-still -o image.jpg

copy:
	scp mhear22@192.168.20.93:/home/mhear22/pitraincamera/images/pic.jpeg .


build:
	docker build . -t pitrain -t ${dockeraddress}

debug: build
	docker run --env-file .env -it pitrain /bin/sh

pretend: build
	docker run --env-file .env -it pitrain

aws-login:
	aws ecr get-login-password --region ap-southeast-2 | docker login --username AWS --password-stdin ${dockeraddress}

publish: build aws-login
	docker push ${dockeraddress}