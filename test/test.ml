open! Core

let%expect_test _ =
  let print_io test_string =
    let actual_length = String.length test_string in
    let input =
      Bigstring.of_string
        (test_string ^ String.make ((64 - (actual_length mod 64)) mod 64) ' ')
    in
    assert (Bigstring.length input mod 64 = 0);
    let indices = Bigarray.Array1.create Int64 C_layout (Bigstring.length input) in
    let n_indices =
      Simd_sexp.extract_structural_indices
        ~input
        ~output:indices
        ~output_index:0
        ~start_offset:0
    in
    let indices = Bigarray.Array1.sub indices 0 n_indices in
    let n_set = Int.Hash_set.create () in
    for i = 0 to n_indices - 1 do
      Hash_set.add n_set (Int64.to_int_exn indices.{i})
    done;
    printf "%s\n" test_string;
    for i = 0 to actual_length - 1 do
      if Hash_set.mem n_set i then printf "^" else printf "."
    done;
    printf "\n";
    (* Stage 2: check the state machine *)
    let state =
      Simd_sexp.State.create ~direct_emit:(fun sexp -> printf !"> %{Sexp#hum}\n" sexp)
    in
    Simd_sexp.State.process_all state input indices;
    printf "\n\n"
  in
  print_io {|foo|};
  print_io {|foo bar|};
  print_io {|foo   bar|};
  [%expect
    {|
    foo
    ^..
    > foo


    foo bar
    ^..^^..
    > foo
    > bar


    foo   bar
    ^..^..^..
    > foo
    > bar |}];
  print_io {|(foo   bar)|};
  print_io {|(fo\o   bar)|};
  print_io {|"()"|};
  print_io {|" "|};
  print_io {|"fo\"o"|};
  print_io {|fo\"o"|};
  [%expect
    {|
    (foo   bar)
    ^^..^..^..^
    > (foo bar)


    (fo\o   bar)
    ^^...^..^..^
    > ("fo\\o" bar)


    "()"
    ^..^
    > "()"


    " "
    ^.^
    > " "


    "fo\"o"
    ^.....^
    > "fo\"o"


    fo\"o"
    ^..^.^
    > "fo\\"
    > o |}];
  print_io {|(foo "bar \"x\" baz" quux      )(foo "bar \"x\" baz" quux      )|};
  [%expect
    {|
    (foo "bar \"x\" baz" quux      )(foo "bar \"x\" baz" quux      )
    ^^..^^.............^.^...^.....^^^..^^.............^.^...^.....^
    > (foo "bar \"x\" baz" quux)
    > (foo "bar \"x\" baz" quux) |}];
  print_io
    {|(foo "bar \"x\" baz" quux      )(foo "bar \"x\" baz" quux      )(foo "bar \"x\" baz" quux      )(foo "bar \"x\" baz" quux      )|};
  [%expect
    {|
    (foo "bar \"x\" baz" quux      )(foo "bar \"x\" baz" quux      )(foo "bar \"x\" baz" quux      )(foo "bar \"x\" baz" quux      )
    ^^..^^.............^.^...^.....^^^..^^.............^.^...^.....^^^..^^.............^.^...^.....^^^..^^.............^.^...^.....^
    > (foo "bar \"x\" baz" quux)
    > (foo "bar \"x\" baz" quux)
    > (foo "bar \"x\" baz" quux)
    > (foo "bar \"x\" baz" quux) |}]
;;

(* print_io
 *   {| (foo "bar \"x\" baz" quux      )(foo "bar \"x\" baz" quux      )(foo "bar \"x\" baz" quux      )|};
 * [%expect
 *   {|
 *    (foo "bar \"x\" baz" quux      )(foo "bar \"x\" baz" quux      )(foo "bar \"x\" baz" quux      )
 *   ^^^..^^.............^^^...^.....^^^..^^.............^^^...^.....^^^..^^.............^^^...^.....^
 *   > (foo "bar \"x\" baz" quux)
 *   > (foo "bar \"x\" baz" quux)
 *   > (foo "bar \"x\" baz" quux) |}];
 * print_io
 *   {|xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy|};
 * [%expect
 *   {|
 *   xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy
 *   ^................................................................................................
 *   > xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy |}];
 * print_io
 *   {|xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx  yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy|};
 * [%expect
 *   {|
 *   xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx  yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy
 *   ^..............................................................^.^...............................
 *   > xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
 *   > yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy |}];
 * print_io {|(                                                              "ab")|};
 * [%expect
 *   {|
 *   (                                                              "ab")
 *   ^^.............................................................^..^^
 *   > (ab) |}] *)
