# TRT Assistente Vivo — Windows ARM64

Assistente pessoal flutuante, desenvolvido em **Tauri 2 + Rust + React + TypeScript**, para Windows 11 ARM64.

## Recursos
- Avatar wireframe animado e janela flutuante sobre a área de trabalho
- Conversa por IA em português brasileiro
- Síntese de voz e microfone quando compatíveis com WebView2
- Seleção de provedores compatíveis com OpenAI API; pode conectar a um serviço localhost já em execução
- Diagnósticos e navegação assistida com aprovação explícita
- Bancos de dados SQLite e credenciais isoladas da Central TRT-Info original

## Segurança e isolamento
Esta cópia pública contém código-fonte, não inclui chaves API, banco de dados pessoal, sessões de chat, perfis de navegador, credenciais nem configurações privadas. O projeto original de localhost não deve ser substituído por este projeto. Não há inicialização automática com o Windows.

Ao clonar, configure seus próprios provedores e chaves no aplicativo; chaves não acompanham o repositório.

## Build (Windows ARM64)
Requisitos: Node.js, npm, Rust com target aarch64-pc-windows-msvc, SDK e toolchain ARM64 e LLVM/Clang.

    npm install
    npm run build
    npm run lint
    npm run tauri -- build --target aarch64-pc-windows-msvc --bundles nsis

Dependências de build Windows ARM64 e ferramentas do sistema podem exigir configuração adicional do ambiente. A versão executável foi compilada e testada no Windows ARM64 de desenvolvimento; o código publicado é uma cópia limpa sem artefatos pessoais.

## Estrutura
- src/: interface, avatar e voz
- src-tauri/: comandos nativos Rust, SQLite, permissões e browser supervisionado
- scripts/: auxiliares de navegador/MCP

## Observações
O módulo compacto permite conversar. Ações de Windows são executadas no painel supervisionado mediante revisão e autorização; não há execução arbitrária de shell, alteração automática de BIOS ou submissão automática de formulários.

**Projeto de Thales Renato Teracin.**

## Executável

O `.exe` e o instalador ARM64 permanecem na máquina de desenvolvimento, em `C:\\TRT-Assistente-Vivo-Standalone\\bin` e `C:\\TRT-Assistente-Vivo-Standalone\\installer`. O GitHub contém a cópia do código-fonte; o binário **não foi publicado neste repositório**. Compile com as instruções acima para gerar sua própria versão.
