open! Core

let sample_sexp_contents =
  {|
(executable
 (name benches)
 (libraries core core_bench core_unix.command_unix core_unix.sys_unix simd_sexp))
|}
;;

let parsexp_bench_test =
  Core_bench.Bench.Test.create_with_initialization ~name:"parsexp" (fun `init () ->
      Parsexp.Many.parse_string sample_sexp_contents)
;;

let simd_sexp_bench_test =
  Core_bench.Bench.Test.create_with_initialization ~name:"simd_sexp" (fun `init () ->
      Simd_sexp.of_string_many sample_sexp_contents)
;;

let command = Core_bench.Bench.make_command [ parsexp_bench_test; simd_sexp_bench_test ]
let () = Command_unix.run command
