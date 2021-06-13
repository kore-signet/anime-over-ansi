require "stumpy_png"
require "./color_diff.cr"

color_map = ColorMap.new "color_data.json"

include StumpyCore

def subtract_rgb(r,g,b,ar,ag,ab)
  {r.to_i - ar.to_i, g.to_i - ag.to_i, b.to_i - ab.to_i}
end

# rgb + delta of each value from subtract_rgb
def calc_error(fraction,pixel,dr,dg,db)
  r,g,b = pixel.to_rgb8
  r,g,b = r.to_f64,g.to_f64,b.to_f64
  RGBA.from_rgb8 (r + dr * fraction).round.clamp(0,255), (g + dg * fraction).round.clamp(0,255), (b + db * fraction).round.clamp(0,255)
end

canvas = StumpyPNG.read(ARGV[0])

(canvas.height-1).times do |y|
  (canvas.width-1).times do |x|
    # puts "#{x},#{y}"
    old_pixel = canvas[x,y].to_rgb8
    new_pixel = color_map.ansi_to_rgb[color_map.find_closest_ansi *old_pixel]
    canvas[x,y] = RGBA.from_rgb8 *new_pixel
    quant_error = subtract_rgb *old_pixel, *new_pixel

    canvas[x+1, y] = calc_error 7/16, canvas[x+1, y], *quant_error
    canvas[x-1, y+1] = calc_error 3/16, canvas[x-1, y+1], *quant_error
    canvas[x, y+1] = calc_error 5/16, canvas[x, y+1], *quant_error
    canvas[x+1, y+1] = calc_error 3/16, canvas[x+1,y+1], *quant_error
  end
end

StumpyPNG.write(canvas,ARGV[0])
