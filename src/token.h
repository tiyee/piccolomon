#ifndef TOKEN_H
#define TOKEN_H
#include<iosfwd>
class Token{
protected:
    const unsigned int _line;
   const  unsigned int _pos;
   const  unsigned int _length;
public:
    Token(unsigned int line,unsigned int pos,unsigned int length):_line(line),_pos(pos),_length(length){};
    virtual void writeAsHtml(std::ostream&) =0;
    virtual void writeToken(std::ostream& out) =0;
};

#endif // TOKEN_H
