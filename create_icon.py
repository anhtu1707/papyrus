from PIL import Image, ImageDraw, ImageFont, ImageFilter
import math

# Master canvas size
SIZE = 1024
master = Image.new('RGBA', (SIZE, SIZE), (0, 0, 0, 0))

# 1. Draw outer soft shadow
shadow = Image.new('RGBA', (SIZE, SIZE), (0, 0, 0, 0))
s_draw = ImageDraw.Draw(shadow)
pad = 64
s_rect = [pad, pad + 16, SIZE - pad, SIZE - pad + 16]
s_draw.rounded_rectangle(s_rect, radius=210, fill=(20, 12, 6, 90))
shadow = shadow.filter(ImageFilter.GaussianBlur(radius=28))
master.paste(shadow, (0, 0), shadow)

# 2. Draw card base (squircle / rounded rectangle)
card = Image.new('RGBA', (SIZE, SIZE), (0, 0, 0, 0))
c_draw = ImageDraw.Draw(card)
rect = [pad, pad, SIZE - pad, SIZE - pad]
radius = 200

# Base parchment fill with radial/linear gradient
for y in range(pad, SIZE - pad):
    ratio = (y - pad) / (SIZE - 2 * pad)
    # Gradient from warm ivory cream #FAF6ED (250, 246, 237) to warm parchment #E5D7BD (229, 215, 189)
    r = int(250 * (1 - ratio) + 225 * ratio)
    g = int(246 * (1 - ratio) + 212 * ratio)
    b = int(237 * (1 - ratio) + 185 * ratio)
    c_draw.line([(pad, y), (SIZE - pad, y)], fill=(r, g, b, 255))

# Mask card to rounded rectangle
mask = Image.new('L', (SIZE, SIZE), 0)
m_draw = ImageDraw.Draw(mask)
m_draw.rounded_rectangle(rect, radius=radius, fill=255)

card_masked = Image.new('RGBA', (SIZE, SIZE), (0, 0, 0, 0))
card_masked.paste(card, (0, 0), mask)

# 3. Outer antique bronze rim
rim_draw = ImageDraw.Draw(card_masked)
rim_draw.rounded_rectangle(rect, radius=radius, outline=(140, 105, 65, 230), width=10)

# Inner hairline frame
inset = pad + 32
inner_rect = [inset, inset, SIZE - inset, SIZE - inset]
rim_draw.rounded_rectangle(inner_rect, radius=radius - 24, outline=(185, 160, 125, 180), width=4)

# Corner ornamental dots
dot_r = 8
corners = [
    (inset + 28, inset + 28),
    (SIZE - inset - 28, inset + 28),
    (inset + 28, SIZE - inset - 28),
    (SIZE - inset - 28, SIZE - inset - 28)
]
for cx, cy in corners:
    rim_draw.ellipse([cx - dot_r, cy - dot_r, cx + dot_r, cy + dot_r], fill=(140, 105, 65, 200))

# 4. Monogram "PRS" in rich deep espresso brown with embossed letterpress effect
font_size = 320
font_path = "C:/Windows/Fonts/georgiab.ttf"
try:
    font = ImageFont.truetype(font_path, font_size)
except:
    font = ImageFont.truetype("C:/Windows/Fonts/arialbd.ttf", font_size)

text = "PRS"

# Measure text
bbox = font.getbbox(text)
text_w = bbox[2] - bbox[0]
text_h = bbox[3] - bbox[1]
tx = (SIZE - text_w) // 2 - bbox[0]
ty = (SIZE - text_h) // 2 - bbox[1] - 10

# Deboss effect:
# Bottom-right highlight (warm light cream)
rim_draw.text((tx + 4, ty + 5), text, font=font, fill=(255, 252, 245, 180))
# Top-left shadow (soft dark imprint)
rim_draw.text((tx - 3, ty - 3), text, font=font, fill=(35, 18, 8, 120))
# Main deep espresso letterpress color: #2C1708 (44, 23, 8)
rim_draw.text((tx, ty), text, font=font, fill=(44, 23, 8, 255))

# Subtle subtitle "PAPYRUS" below monogram
sub_size = 54
try:
    sub_font = ImageFont.truetype("C:/Windows/Fonts/georgia.ttf", sub_size)
except:
    sub_font = ImageFont.truetype("C:/Windows/Fonts/arial.ttf", sub_size)

sub_text = "• B R O W S E R •"
s_bbox = sub_font.getbbox(sub_text)
s_w = s_bbox[2] - s_bbox[0]
s_h = s_bbox[3] - s_bbox[1]
stx = (SIZE - s_w) // 2 - s_bbox[0]
sty = ty + text_h + 38

rim_draw.text((stx + 1, sty + 2), sub_text, font=sub_font, fill=(255, 252, 245, 160))
rim_draw.text((stx, sty), sub_text, font=sub_font, fill=(110, 75, 45, 220))

# Composite master
master.paste(card_masked, (0, 0), card_masked)

# 5. Export multi-size ICO
sizes = [(256, 256), (128, 128), (64, 64), (48, 48), (32, 32), (16, 16)]
ico_images = []
for s in sizes:
    resized = master.resize(s, Image.Resampling.LANCZOS)
    ico_images.append(resized)

out_path = "F:/Benali/papyrus/assets/papyrus.ico"
ico_images[0].save(
    out_path,
    format='ICO',
    sizes=sizes,
    append_images=ico_images[1:]
)
print("Successfully generated luxury PRS icon at", out_path)
