#include "h.h"

H::H(Level lv, std::string txt, unsigned int line, unsigned int pos,
     unsigned int length)
    : Token(line, pos, length), _level(lv), _txt(txt) {}
