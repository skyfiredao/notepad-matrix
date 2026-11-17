use std::{cell::RefCell, collections::{VecDeque, hash_map::RandomState}, hash::BuildHasher, rc::Rc, time::{Duration, Instant}};

use gpui::{AnyElement, App, Bounds, ContentMask, IntoElement, Pixels, ShapedLine, Styled, TextAlign, TextRun, Window, canvas, fill, font, point, px, rgb};

const MAX_LINES: usize = 128;
const MAX_LINE_CHARS: usize = 1024;

pub(super) struct Terminal {
    state: Rc<RefCell<State>>,
}

impl Terminal {
    pub(super) fn new() -> Self {
        let seed = RandomState::new().hash_one(0u8) as u32;
        let scene = match seed % 6 {
            0 => Scene::Cargo,
            1 => Scene::Cmake,
            2 => Scene::Docker,
            3 => Scene::Maven,
            4 => Scene::Packages,
            _ => Scene::Download,
        };
        let mut state = State {
            lines: VecDeque::new(),
            scene,
            step: 0,
            percent: 0,
            last_tick: None,
            random: seed.max(1),
            ticks: 0,
        };
        state.push_command();
        Self { state: Rc::new(RefCell::new(state)) }
    }

    pub(super) fn render(&mut self, _: &mut Window) -> AnyElement {
        let state = self.state.clone();
        canvas(|_, _, _| (), move |bounds, _, window, cx| {
            state.borrow_mut().paint(bounds, window, cx);
        }).size_full().into_any_element()
    }
}

#[derive(Clone, Copy)]
enum Scene {
    Cargo,
    Cmake,
    Docker,
    Maven,
    Packages,
    Download,
}

#[derive(Clone, Copy)]
enum Step {
    Line(&'static str, &'static str),
    Progress(&'static str, &'static str, &'static str),
}

impl Scene {
    fn command(self) -> &'static str {
        match self {
            Self::Cargo => "cargo build --release --verbose",
            Self::Cmake => "cmake --build build --parallel 8 --verbose",
            Self::Docker => "docker build -t orbit-demo:latest .",
            Self::Maven => "mvn -B -T 2C -DskipTests=false clean package",
            Self::Packages => "apt install clang cmake ninja-build",
            Self::Download => "curl -LO https://downloads.example.invalid/demo-assets.tar.zst",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Cargo => Self::Cmake,
            Self::Cmake => Self::Docker,
            Self::Docker => Self::Maven,
            Self::Maven => Self::Packages,
            Self::Packages => Self::Download,
            Self::Download => Self::Cargo,
        }
    }

    fn steps(self) -> &'static [Step] {
        match self {
            Self::Cargo => &[
                Step::Line("    Updating", " registry index"),
                Step::Progress(" Downloading", "libc v0.2.183", "MiB/s"),
                Step::Progress(" Downloading", "serde v1.0.228", "MiB/s"),
                Step::Line("   Compiling", " libc v0.2.183"),
                Step::Line("   Compiling", " serde v1.0.228"),
                Step::Line("   Compiling", " futures-core v0.3.31"),
                Step::Line("   Compiling", " scene-renderer v0.8.2"),
                Step::Line("     Running", r#" `env CARGO_CFG_TARGET_ARCH=aarch64 CARGO_PKG_VERSION=0.8.2 RUSTFLAGS='-C target-cpu=native -C link-arg=-Wl,--as-needed' rustc --crate-name scene_renderer --edition=2024 src/lib.rs --crate-type lib --emit=dep-info,metadata,link -C opt-level=3 -C codegen-units=1 -C debuginfo=0 --cfg feature="default" --cfg feature="simd" -L dependency=target/release/deps --extern serde=target/release/deps/libserde.rlib --extern futures_core=target/release/deps/libfutures_core.rlib --out-dir target/release/deps`"#),
                Step::Progress("     Linking", "target/release/orbit-demo", ""),
                Step::Line("    Finished", " release [optimized] target(s) in 28.41s"),
            ],
            Self::Cmake => &[
                Step::Line("--", " Configuring done (0.4s)"),
                Step::Line("--", " Generating done (0.1s)"),
                Step::Line("[ 12%]", " Building C object src/window.c.o"),
                Step::Line("[ 25%]", " Building CXX object src/renderer.cpp.o"),
                Step::Line("", "env CCACHE_DIR=/tmp/ccache CXX=clang++ CXXFLAGS='-O3 -DNDEBUG' /usr/bin/clang++ -DORBIT_ENABLE_SIMD=1 -DPLATFORM_LINUX=1 -DRENDER_BACKEND_VULKAN=1 -Iinclude -Isrc -Ithird_party/glm -Ithird_party/stb -isystem /usr/local/include -std=c++20 -O3 -DNDEBUG -Wall -Wextra -Wpedantic -Wconversion -Wshadow -fPIC -ffunction-sections -fdata-sections -march=armv8-a -c src/renderer.cpp -o build/CMakeFiles/scene.dir/src/renderer.cpp.o"),
                Step::Line("[ 37%]", " Building CXX object src/particles.cpp.o"),
                Step::Line("[ 50%]", " Building CXX object src/shaders.cpp.o"),
                Step::Line("[ 62%]", " Linking CXX static library libscene.a"),
                Step::Line("[ 75%]", " Building CXX object src/main.cpp.o"),
                Step::Line("", "/usr/bin/clang++ -O3 -DNDEBUG -Wl,--as-needed -Wl,--gc-sections -Wl,-rpath,/opt/orbit/lib build/CMakeFiles/scene.dir/src/main.cpp.o build/CMakeFiles/scene.dir/src/renderer.cpp.o -Lbuild/lib -L/usr/local/lib -lscene -lvulkan -lpthread -ldl -lm -o build/bin/orbit-demo"),
                Step::Progress("[ 90%]", "Linking CXX executable orbit-demo", ""),
                Step::Line("[100%]", " Built target orbit-demo"),
            ],
            Self::Docker => &[
                Step::Line("[+]", " Building image (1/8)"),
                Step::Line(" =>", " [internal] load build definition from Dockerfile"),
                Step::Line(" =>", " [internal] load metadata for alpine:3.21"),
                Step::Progress(" =>", "[1/5] FROM alpine:3.21", "MiB/s"),
                Step::Line(" =>", " [2/5] WORKDIR /app"),
                Step::Line(" =>", " [3/5] COPY . ."),
                Step::Progress(" =>", "[4/5] RUN npm ci", "MiB/s"),
                Step::Line(" =>", " [5/5] RUN npm run build"),
                Step::Line(" =>", " env NODE_ENV=production CI=true npm_config_cache=/tmp/npm-cache npm run build -- --mode production --minify terser --target es2022 --outDir /app/dist --assetsDir static --sourcemap hidden"),
                Step::Progress(" =>", "exporting layers", "MiB/s"),
                Step::Line(" =>", " naming to docker.io/library/orbit-demo:latest"),
                Step::Line("[+]", " Image built successfully"),
            ],
            Self::Maven => &[
                Step::Line("[INFO]", " Scanning for projects..."),
                Step::Line("[INFO]", " Building orbit-service 1.4.2"),
                Step::Progress("Downloading", "org/apache/commons/commons-lang3/3.17.0/commons-lang3-3.17.0.jar", "MiB/s"),
                Step::Progress("Downloading", "com/fasterxml/jackson/core/jackson-databind/2.18.2/jackson-databind-2.18.2.jar", "MiB/s"),
                Step::Line("[INFO]", " --- maven-resources-plugin:3.3.1:resources (default-resources) @ orbit-service ---"),
                Step::Line("[INFO]", " Copying 8 resources from src/main/resources to target/classes"),
                Step::Line("[DEBUG]", " env JAVA_HOME=/opt/jdk-21 MAVEN_OPTS='-Xms256m -Xmx2048m -XX:+UseG1GC -Dfile.encoding=UTF-8' CLASSPATH=target/classes:lib/commons-lang3-3.17.0.jar:lib/jackson-databind-2.18.2.jar /opt/jdk-21/bin/javac -J-Xmx2048m -J-Dfile.encoding=UTF-8 --release 21 -encoding UTF-8 -parameters -g -Xlint:all --class-path target/classes:lib/commons-lang3-3.17.0.jar:lib/jackson-databind-2.18.2.jar -sourcepath src/main/java -d target/classes src/main/java/com/example/orbit/Application.java src/main/java/com/example/orbit/SceneController.java src/main/java/com/example/orbit/AssetService.java"),
                Step::Line("[INFO]", " Compiling 42 source files with javac [debug release 21] to target/classes"),
                Step::Line("[DEBUG]", " /opt/jdk-21/bin/java -Xms256m -Xmx1024m -XX:+UseG1GC -Duser.timezone=UTC -Dfile.encoding=UTF-8 -Dspring.profiles.active=test -Djava.awt.headless=true -Dsurefire.useFile=false -Dapp.cache.enabled=false -Dapp.render.threads=8 -classpath target/test-classes:target/classes:lib/junit-platform-console-standalone-1.11.4.jar:lib/mockito-core-5.15.2.jar org.apache.maven.surefire.booter.ForkedBooter target/surefire 2026-01-01T10-30-00_001-jvmRun1 surefire-config.tmp surefire-state.tmp"),
                Step::Line("[INFO]", " Tests run: 28, Failures: 0, Errors: 0, Skipped: 0"),
                Step::Line("[INFO]", " Building jar: /workspace/orbit-service/target/orbit-service-1.4.2.jar"),
                Step::Line("[INFO]", " BUILD SUCCESS"),
            ],
            Self::Packages => &[
                Step::Line("Reading", " package lists... Done"),
                Step::Line("Building", " dependency tree... Done"),
                Step::Line("Need to get", " 42.8 MB of archives"),
                Step::Progress("Get:1", "clang-19_19.1.7_arm64.deb", "MiB/s"),
                Step::Progress("Get:2", "cmake_3.31.6_arm64.deb", "MiB/s"),
                Step::Progress("Get:3", "ninja-build_1.12.1_arm64.deb", "MiB/s"),
                Step::Line("Unpacking", " clang-19 (19.1.7) ..."),
                Step::Line("Setting up", " cmake (3.31.6) ..."),
                Step::Line("Setting up", " ninja-build (1.12.1) ..."),
                Step::Line("Processing", " triggers for libc-bin (2.40) ..."),
            ],
            Self::Download => &[
                Step::Line("Connected", " to downloads.example.invalid (HTTPS)"),
                Step::Progress("Receiving", "demo-assets.tar.zst", "MiB/s"),
                Step::Progress("Receiving", "textures-4k.pack", "MiB/s"),
                Step::Progress("Receiving", "audio-samples.zip", "MiB/s"),
                Step::Line("Verifying", " checksums... 3 files verified"),
                Step::Line("Extracting", " demo-assets/textures/"),
                Step::Line("Extracting", " demo-assets/audio/"),
                Step::Line("Ready", " 384 textures, 96 sounds, 12 scenes"),
            ],
        }
    }
}

struct Line {
    text: String,
    prefix: usize,
    columns: usize,
    shaped: Vec<ShapedLine>,
}

impl Line {
    fn row_count(&self, columns: usize) -> usize {
        self.text.chars().count().max(1).div_ceil(columns)
    }

    fn shape(&mut self, columns: usize, window: &Window) {
        if self.columns == columns { return; }
        self.shaped.clear();
        let characters: Vec<_> = self.text.chars().collect();
        let mut offset = 0;
        for chunk in characters.chunks(columns) {
            let text: String = chunk.iter().collect();
            let prefix = self.prefix.saturating_sub(offset).min(text.len());
            offset += text.len();
            let mut runs = Vec::with_capacity(2);
            if prefix > 0 {
                runs.push(TextRun { len: prefix, font: font("Menlo"), color: rgb(0x33d17a).into(), background_color: None, underline: None, strikethrough: None });
            }
            if text.len() > prefix {
                runs.push(TextRun { len: text.len() - prefix, font: font("Menlo"), color: rgb(0xd8d8d8).into(), background_color: None, underline: None, strikethrough: None });
            }
            self.shaped.push(window.text_system().shape_line(text.into(), px(13.), &runs, Some(px(8.))));
        }
        self.columns = columns;
    }
}

struct State {
    lines: VecDeque<Line>,
    scene: Scene,
    step: usize,
    percent: u32,
    last_tick: Option<Instant>,
    random: u32,
    ticks: u32,
}

impl State {
    fn push_command(&mut self) {
        self.push("", format!("build@workstation:~/project$ {}", self.scene.command()));
    }

    fn push(&mut self, prefix: &str, body: String) {
        if self.lines.len() == MAX_LINES { self.lines.pop_front(); }
        let text = format!("{prefix}{body}").chars().take(MAX_LINE_CHARS).collect();
        self.lines.push_back(Line { text, prefix: prefix.len(), columns: 0, shaped: Vec::new() });
    }

    fn next(&mut self, upper: u32) -> u32 {
        self.random ^= self.random << 13;
        self.random ^= self.random >> 17;
        self.random ^= self.random << 5;
        self.random % upper
    }

    fn advance(&mut self, now: Instant, running: bool) {
        if !running { self.last_tick = None; return; }
        let Some(last) = self.last_tick else { self.last_tick = Some(now); return; };
        let elapsed = now.saturating_duration_since(last);
        if elapsed < Duration::from_millis(120) { return; }
        self.last_tick = Some(now);
        if elapsed > Duration::from_millis(500) { return; }
        self.ticks = self.ticks.wrapping_add(1);
        let Some(step) = self.scene.steps().get(self.step).copied() else {
            if self.ticks % 12 == 0 {
                self.scene = self.scene.next();
                self.step = 0;
                self.percent = 0;
                self.push("", String::new());
                self.push_command();
            }
            return;
        };
        match step {
            Step::Progress(prefix, label, unit) => {
                let percent = (self.percent + 8 + self.next(13)).min(100);
                let filled = percent as usize / 5;
                if self.percent > 0 { self.lines.pop_back(); }
                let rate = if unit.is_empty() { String::new() } else { format!("  {}.{} {unit}", 4 + self.next(36), percent % 10) };
                self.push(prefix, format!(" {label} [{}{}] {percent:3}%{rate}", "=".repeat(filled), " ".repeat(20 - filled)));
                if percent == 100 {
                    self.step += 1;
                    self.percent = 0;
                } else {
                    self.percent = percent;
                }
            }
            Step::Line(prefix, body) if self.ticks % 3 == 0 => {
                self.push(prefix, body.into());
                self.step += 1;
            }
            _ => {},
        }
    }

    fn paint(&mut self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        let visible = bounds.intersect(&window.content_mask().bounds).intersect(&window.fully_visible_bounds());
        if visible.size.width <= px(0.) || visible.size.height <= px(0.) || !window.is_visible() {
            self.last_tick = None;
            return;
        }
        let running = objc2::MainThreadMarker::new().is_some_and(|main_thread|
            objc2_app_kit::NSApplication::sharedApplication(main_thread).isActive()) && !cx.reduce_motion();
        self.advance(Instant::now(), running);
        let columns = ((f32::from(bounds.size.width) - 24.) / 8.).floor().clamp(1., 180.) as usize;
        let rows = ((f32::from(bounds.size.height) - 24.) / 18.).floor().clamp(1., 96.) as usize;
        let total_rows: usize = self.lines.iter().map(|line| line.row_count(columns)).sum();
        let skip = total_rows.saturating_sub(rows);
        window.with_content_mask(Some(ContentMask { bounds: visible }), |window| {
            window.paint_quad(fill(visible, rgb(0x000000)));
            let mut row = 0;
            for line in &mut self.lines {
                let count = line.row_count(columns);
                if row + count <= skip || line.text.is_empty() {
                    row += count;
                    continue;
                }
                line.shape(columns, window);
                for part in &line.shaped {
                    if row >= skip {
                        let origin = bounds.origin + point(px(12.), px(12. + (row - skip) as f32 * 18.));
                        if let Err(error) = part.paint(origin, px(18.), TextAlign::Left, None, window, cx) {
                            eprintln!("Terminal animation paint failed: {error}");
                            return;
                        }
                    }
                    row += 1;
                }
            }
            if running { window.request_animation_frame(); }
        });
    }
}
