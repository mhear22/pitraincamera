clone:
	git clone https://github.com/mhear22/pitraincamera.git && cd pitraincamera && make install && make run

install:
	python3 -m pip install -r requirements.txt

setup-pi:
	sudo apt-get install python3-scipy

run:
	cd src/webapp && python3 manage.py runserver 0.0.0.0:8000

listen:
	python3 src/mic/listener.py

pic:
	libcamera-still -o image.jpg

copy:
	scp mhear22@192.168.20.93:/home/mhear22/image.jpg .