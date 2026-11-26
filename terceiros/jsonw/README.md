# jsonW (libjsonw), embutida sem modificação

As ferramentas usam jsonW como formato de tudo: configuração em jsonW-texto (`.jsw.txt`), listas em arquivo coluna (`.jsw`)
e entrada e saída da linha de comando. Quem lê, valida e grava é a libjsonw oficial, ligada estática no binário `wortic`.

- **Origem:** https://github.com/developer-wortic/jsonw, commit `212bcbf180a549f2d4555c5c9e16b8be930a08ce` (16/11/2026),
  libjsonw 1.4.2.
- **O que veio:** `lib/<plataforma>/libjsonw.a` (de `bin/<plataforma>/` do repositório), `sdk/jsonw.h` (a ABI pública),
  `LICENCA.md` e `LICENSE.md`. Nada foi alterado.
- **Licença:** Licença de Uso Sustentável do jsonW (`LICENCA.md`), que permite usar, copiar e distribuir sem modificação e
  criar programas pela ABI publicada com a licença que quiser. O código deste repositório fora desta pasta é MIT.
- **Plataformas:** linux-x86_64, linux-arm64, macos-arm64 e windows-x86_64 (MinGW). Para outra, defina `JSONW_LIB_DIR`
  com a pasta de uma `libjsonw.a` compatível.

Conferência (sha256):

```
3506c9dd2a231b9cf14c2121f96c8d699c59e923006c5057d27d791519f793e0  lib/linux-x86_64/libjsonw.a
d8b1d6a3e591fff1fb2f8cc4118b365f8af88e839af5b1d756621ff2400908df  lib/linux-arm64/libjsonw.a
ee6f5ab5a5520f572c5a2d9d7b976f708334fefb89982d6aa8c6cc0197fefe77  lib/macos-arm64/libjsonw.a
09c944ad107276761c4a53030f86fd23a8feca122aaae605e90eb506a1a4e69a  lib/windows-x86_64/libjsonw.a
1516807d7d70de4ee7ea9e63ef7c4c8c09500c2b7f218c2e8ad20067a97455f8  sdk/jsonw.h
```

Para atualizar: copie os mesmos arquivos de uma versão nova do repositório jsonw, troque o commit e os hashes acima e rode
`cargo test --workspace`.
