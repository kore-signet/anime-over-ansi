require "stumpy_png"
require "./color_diff.cr"
require "colorize"

canvas = StumpyPNG.read(ARGV[0])
file = File.new(ARGV[0]+".txt", "w")

file << "\u001b[0;0f"

color_map = ColorMap.new "color_data.json"

(canvas.height-1).times.step(2).each do |y|
  (canvas.width-1).times do |x|
      upper = Colorize::Color256.new color_map.find_closest_ansi *canvas[x,y].to_rgb8
      lower = Colorize::Color256.new color_map.find_closest_ansi *canvas[x,y+1].to_rgb8
      file << "▀".colorize.fore(upper).back(lower).to_s
    end
    file << "\n"
end
file << "\n"
