{ lib, hostPlatform, buildPackages
, writeScript, linkFarm
, crates
, mkTask, mkLoader
, embedDebugInfo
, seL4RustTargetInfoWithConfig
, worldConfig
, callPackage
, seL4ForBoot

, mkCapDLLoader
, mkSmallCapDLLoader
, mkCapDLSpec
}:

let
  mk = { rootTask, isSupported, canAutomate ? false, automateTimeout ? defaultTimeout, extraLinks ? [] }: rec {
    inherit rootTask isSupported canAutomate;

    loader = mkLoader {
      appELF = rootTask.elf;
    };

    qemuCmd = worldConfig.mkQemuCmd (if worldConfig.qemuCmdRequiresLoader then loader.elf else rootTask.elf);

    simulate = writeScript "run.sh" ''
      #!${buildPackages.runtimeShell}
      exec ${lib.concatStringsSep " " qemuCmd} "$@"
    '';

    symbolizeRootTaskBacktrace = writeScript "x.sh" ''
      #!${buildPackages.runtimeShell}
      exec ${buildPackages.this.sel4-symbolize-backtrace}/bin/sel4-symbolize-backtrace -f ${rootTask.elf} "$@"
    '';

    links = linkFarm "links" ([
      { name = "simulate"; path = simulate; }
    ] ++ lib.optionals worldConfig.qemuCmdRequiresLoader [
      { name = "loader.elf"; path = loader.elf; }
    ] ++ [
      { name = "kernel.elf"; path = "${seL4ForBoot}/bin/kernel.elf"; }
      { name = "root-task.elf"; path = rootTask.elf; }
      { name = "symbolize-root-task-backtrace"; path = symbolizeRootTaskBacktrace; }
    ] ++ extraLinks ++ (rootTask.extraLinks or []));
    # ]);

    automate =
      assert canAutomate;
      assert automateTimeout != null;
      automateQemuBasic {
        inherit simulate;
        timeout = automateTimeout;
      };
  };

  mkCapDLRootTask =
    { script
    , config
    , passthru
    , small ? false
    }:
    let
    in lib.fix (self: with self;
      {
        inherit script config;
        spec = mkCapDLSpec {
          inherit script config;
        };
        extraLinks = [
          { name = "cdl"; path = spec; }
        ] ++ lib.optionals (!small) [
          # { name = "x.elf"; path = self.loader.split.full; }
          { name = "x.elf"; path = self.loader.split.full; }
        ];
      }
      // (if small then {
        loader = mkSmallCapDLLoader spec.cdl;
        elf = loader.elf;
      } else {
        loader = mkCapDLLoader spec.cdl;
        elf = loader;
      })
      // passthru
    );

  haveFullRuntime = hostPlatform.isAarch64 || hostPlatform.isx86_64;
  haveMinimalRuntime = haveFullRuntime;
  haveKernelLoader = hostPlatform.isAarch64;

  automateQemuBasic = { simulate, timeout }:
    writeScript "automate-qemu" ''
      #!${buildPackages.runtimeShell}
      set -eu

      script=${simulate}
      timeout_=${toString timeout}

      echo "running '$script' with timeout ''${timeout_}s"

      # the odd structure of this next part is due to bash's limitations on
      # pipes, process substition, and coprocesses.

      coproc $script < /dev/null
      result=$( \
        timeout $timeout_ bash -c \
          'head -n1 <(bash -c "tee >(cat >&2)" | grep -E -a --line-buffered --only-matching "TEST_(PASS|FAIL)")' \
          <&''${COPROC[0]} \
          || true
      )
      kill $COPROC_PID

      echo "result: '$result'"
      [ "$result" == "TEST_PASS" ]
    '';

  defaultTimeout = 30;

in rec {

  all = [
    examples.root-task.example-root-task
    examples.root-task.example-root-task-without-runtime
    tests.loader
    tests.core-libs
    tests.config
    tests.tls
    tests.injected-phdrs
    tests.backtrace
    tests.panicking.abort.withAlloc
    tests.panicking.abort.withoutAlloc
    tests.panicking.unwind.withAlloc
    tests.panicking.unwind.withoutAlloc
  ];

  supported = lib.filter (instance: instance.isSupported) all;

  examples = {
    root-task = {
      example-root-task-without-runtime = mk {
        rootTask = mkTask {
          rootCrate = crates.example-root-task-without-runtime;
          release = false;
          rustTargetInfo = seL4RustTargetInfoWithConfig { minimal = true; };
        };
        isSupported = haveMinimalRuntime;
        canAutomate = true;
      };

      example-root-task = mk {
        rootTask = mkTask {
          rootCrate = crates.example-root-task;
          release = false;
        };
        isSupported = haveFullRuntime;
        canAutomate = true;
      };
    };
  };

  tests = {
    loader = mk {
      rootTask = mkTask {
        rootCrate = crates.tests-root-task-loader;
        release = false;
      };
      isSupported = haveKernelLoader && haveFullRuntime;
      canAutomate = true;
    };

    core-libs = mk {
      rootTask = mkTask {
        rootCrate = crates.tests-root-task-core-libs;
        release = false;
      };
      isSupported = haveFullRuntime;
      canAutomate = true;
    };

    config = mk {
      rootTask = mkTask {
        rootCrate = crates.tests-root-task-config;
        release = false;
      };
      isSupported = haveFullRuntime;
      canAutomate = true;
    };

    tls = mk {
      rootTask = mkTask {
        rootCrate = crates.tests-root-task-tls;
        release = false;
      };
      isSupported = haveFullRuntime;
      canAutomate = true;
    };

    injected-phdrs = mk {
      rootTask = mkTask {
        rootCrate = crates.tests-root-task-injected-phdrs;
        release = true;
        injectPhdrs = true;
      };
      isSupported = haveFullRuntime;
      canAutomate = true;
    };

    backtrace = mk {
      rootTask =
        let
          orig = mkTask {
            rootCrate = crates.tests-root-task-backtrace;
            release = false;
          };
        in {
          elf = embedDebugInfo orig.elf;
          inherit orig;
        };
      isSupported = haveFullRuntime;
      canAutomate = true;
    };

    panicking =
      let
        alloc = {
          withAlloc = [ "alloc" ];
          withoutAlloc = [];
        };
        panicStrategy = {
          unwind = null;
          abort = null;
        };
      in
        lib.flip lib.mapAttrs panicStrategy
          (panicStrategyName: _:
            lib.flip lib.mapAttrs alloc
              (_: allocFeatures: mk {
                rootTask = mkTask {
                  rootCrate = crates.tests-root-task-panicking;
                  release = false;
                  features = allocFeatures ++ [ "panic-${panicStrategyName}" ];
                  extraProfile = {
                    panic = panicStrategyName;
                  };
                };
                isSupported = haveFullRuntime;
                canAutomate = panicStrategyName == "unwind";
              }));

    c = callPackage ./c.nix {
      inherit mk;
    };

    capdl = {
      threads = mk {
        rootTask = mkCapDLRootTask rec {
          # small = true;
          script = ../../../../../crates/tests/capdl/threads/cdl.py;
          config = {
            components = {
              example_component.image = passthru.test.elf;
            };
          };
          passthru = {
            test = mkTask {
              rootCrate = crates.tests-capdl-threads-components-test;
            };
          };
        };
        isSupported = haveFullRuntime;
        canAutomate = true;
      };
      utcover = mk {
        rootTask = mkCapDLRootTask rec {
          # small = true;
          script = ../../../../../crates/tests/capdl/utcover/cdl.py;
          config = {
            components = {
              example_component.image = passthru.test.elf;
            };
          };
          passthru = {
            test = mkTask {
              rootCrate = crates.tests-capdl-utcover-components-test;
              release = false;
            };
          };
        };
        isSupported = haveFullRuntime;
        canAutomate = true;
      };
    };
  };

}
