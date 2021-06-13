require "socket"
require "json"
require "colorize"
require "./color_diff.cr"

tx = Channel(String).new

ENV["PORT"] ||= "8023"

server = TCPServer.new "0.0.0.0", ENV["PORT"].to_i
sockets = Array(TCPSocket).new

frames = File.read_lines("frames.txt").reverse
buffered_frames = Array(String).new

def handle_client(socket : TCPSocket, sockets : Array(TCPSocket))
  sockets << socket

  while line = socket.gets chomp: false
  end

  sockets.delete socket
end

spawn do
  while socket = server.accept?
    spawn handle_client(socket,sockets)
  end
end

puts "buffering frames for 5s before starting"
start = Time.monotonic
while (Time.monotonic - start).total_seconds < 20.0
  buffered_frames.push File.read frames.pop
end

puts "#{buffered_frames.size} frames buffered"
sleep 1

while true
  start = Time.monotonic

  f = buffered_frames.pop
  sockets.each do |s|
    s << f
  end

  while (Time.monotonic - start).total_milliseconds < 41.1
    buffered_frames.push File.read frames.pop
  end
  sleep (Time.monotonic - start).total_seconds
end
