import pyaudio
import wave
from scipy.io import wavfile
import numpy as np
from statistics import mean
import os

form_1 = pyaudio.paInt16 # 16-bit resolution
chans = 1 # 1 channel
samp_rate = 44100 # 44.1kHz sampling rate
chunk = 4096 # 2^12 samples for buffer
record_secs = 0.1 # seconds to record
wav_output_filename = 'test1.wav' # name of .wav file


class Listener:
    def __init__(self):
        self.trigger_sensitivity = os.environ.get("LISTENING_SENSITIVITY", 1000)
        self.audio = pyaudio.PyAudio()
        self.devices = []
        for device_index in range(self.audio.get_device_count()):
            device_info = self.audio.get_device_info_by_index(device_index)
            device_info['index'] = device_index
            self.devices.append(device_info)

    def measure_wave(self, filename):
        rate, data = wavfile.read(filename)
        smol_list = []
        for i in range(0, len(data), 1000):
            window = data[i:i+1000]
            smol_list.append(mean(window))

        for index, item in enumerate(smol_list):
            try: 
                next = smol_list[index + 1]
                if((item + self.trigger_sensitivity) < next):
                    return True
            except:
                pass
        return False
    
    def record(self, device_index):
        stream = self.audio.open(
            format = form_1,
            rate = samp_rate,
            channels = chans,
            input_device_index = device_index,
            input = True,
            frames_per_buffer=chunk
        )

        frames = []
        for _ in range(0,int((samp_rate/chunk)*record_secs)):
            data = stream.read(chunk)
            frames.append(data)
        stream.stop_stream()
        stream.close()

        # save the audio frames as .wav file
        wavefile = wave.open(wav_output_filename,'wb')
        wavefile.setnchannels(chans)
        wavefile.setsampwidth(self.audio.get_sample_size(form_1))
        wavefile.setframerate(samp_rate)
        wavefile.writeframes(b''.join(frames))
        wavefile.close()
        return wav_output_filename

    def listen(self):
        device = [(item) for item in self.devices if item.get('name') == 'Astro A50 Voice'][0]

        while(True):
            self.trigger_sensitivity = os.environ.get("LISTENING_SENSITIVITY", 1000)
            filename = self.record(device.get('index'))
            train_detected = self.measure_wave(filename)
            if(train_detected):
                print("LOUD!")




if __name__ == '__main__':
    Listener().listen()