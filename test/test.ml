let%expect_test _ =
  print_endline (Simd_sexp.hello_world ());
  [%expect {| hello, world! |}]
;;
