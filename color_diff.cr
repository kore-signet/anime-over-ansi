require "json"

class ColorMap
  property color_map : Hash({Float64,Float64,Float64},UInt8)
  property color_cache : Hash({UInt8,UInt8,UInt8},UInt8) # cache colors to avoid doing more maths than necessary
  property ansi_to_rgb : Array({UInt8,UInt8,UInt8})

  def initialize (file)
    color_data = JSON.parse(File.read(file)).as_a
    @color_map = Hash({Float64,Float64,Float64},UInt8).new
    @ansi_to_rgb = Array({UInt8,UInt8,UInt8}).new 256, {0_u8,0_u8,0_u8}
    @color_cache = Hash({UInt8,UInt8,UInt8},UInt8).new

    color_data.each do |c|
      r = c["rgb"]["r"].as_i
      g = c["rgb"]["g"].as_i
      b = c["rgb"]["b"].as_i

      xyz = rgb_to_xyz(r,g,b)
      lab = xyz_to_lab(*xyz)
      @color_cache[{r.to_u8,g.to_u8,b.to_u8}] = c["colorId"].as_i.to_u8
      @color_map[{lab[0],lab[1],lab[2]}] = c["colorId"].as_i.to_u8
      @ansi_to_rgb[c["colorId"].as_i] = {r.to_u8,g.to_u8,b.to_u8}
    end
  end

  def get_hex_color(s : String)
    if s.empty?
      return Colorize::ColorRGB.new 255,255,255
    end

    r = s[1..2].to_u8 base: 16
    g = s[3..4].to_u8 base: 16
    b = s[5..6].to_u8 base: 16

    Colorize::Color256.new find_closest_ansi(r,g,b)
  end

  def find_closest_ansi (r,g,b)
    res = @color_cache[{r,g,b}]?
    if res.nil?
      res = calculate_closest_ansi r, g, b
      @color_cache[{r,g,b}] = res
      res
    else
      res.not_nil!
    end
  end

  def calculate_closest_ansi (r,g,b)
    r = r.to_f64
    g = g.to_f64
    b = b.to_f64

    xyz = rgb_to_xyz(r,g,b)
    l, a, b = xyz_to_lab(*xyz)

    (@color_map.map do |compare,ansi|
      cl, ca, cb = compare
      {((l - cl) ** 2) + ((a - ca) ** 2) + ((b - cb) ** 2),ansi}
    end).to_a.sort_by { |a| a[0] }[0][1]
  end

  def rgb_to_xyz(r,g,b)
    r /= 255
    g /= 255
    b /= 255

    r = if r > 0.04045
      ((r + 0.055) / 1.055) ** 2.4
    else
      r / 12.92
    end

    g = if g > 0.04045
      ((g + 0.055) / 1.055) ** 2.4
    else
      g / 12.92
    end

    b = if b > 0.04045
      ((b + 0.055) / 1.055) ** 2.4
    else
      b / 12.92
    end

    r *= 100
    g *= 100
    b *= 100

    x = r * 0.4124 + g * 0.3576 + b * 0.1805
    y = r * 0.2126 + g * 0.7152 + b * 0.0722
    z = r * 0.0193 + g * 0.1192 + b * 0.9505

    return {x,y,z}
  end

  def xyz_to_lab(x,y,z)
    ref_x = 95.047
    ref_y = 100.000
    ref_z = 108.883

    x /= ref_x
    y /= ref_y
    z /= ref_z

    x = if x > 0.008856
      x ** (1/3)
    else
      (7.787 * x) + 16 / 116
    end

    y = if y > 0.008856
      y ** (1/3)
    else
      (7.787 * y) + 16 / 116
    end

    z = if z > 0.008856
      z ** (1/3)
    else
      (7.787 * z) + 16 / 116
    end

    l = (116 * y) - 16
    a = 500 * (x - y)
    b = 200 * (y - z)
    return {l,a,b}
  end
end
