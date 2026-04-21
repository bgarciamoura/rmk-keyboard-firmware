# NOTICE

## Bongo Cat sprites

Os sprites em `dongle/src/assets/bongo/` são derivados do repositório
[sidharth-458/Bongo_cat-32](https://github.com/sidharth-458/Bongo_cat-32)
(arquivo `sketch_sep25a.ino`), via o script `tools/extract_bongo_sprites.py`
deste repo.

A arte original do Bongo Cat é creditada pelo repo-fonte a **@pixelbunny**.
O repositório upstream **não declara licença** — os sprites são redistribuídos
aqui apenas para uso pessoal/educacional não-comercial, com todos os créditos
preservados. Caso o autor original solicite remoção, os arquivos serão
imediatamente retirados.

Layout dos sprites: 64×64 @ 1 bpp, formato `Adafruit_GFX::drawBitmap`
(row-major, 8 pixels por byte, MSB-first). 13 frames extraídos (0..12):

- Frames 0..8: animação idle (cabeça balançando)
- Frame 10: pose base com patas levantadas
- Frame 11: pata esquerda batendo
- Frame 12: pata direita batendo
- Frame 9: ambas as patas batendo
